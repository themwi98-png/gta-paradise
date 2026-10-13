//! Minimal ScriptHookV binding. ScriptHookV.dll exports C++-mangled
//! functions; they are imported by name with raw-dylib, so no SDK or import
//! library is needed. On x64 the C++ and C calling conventions are the same.

use std::ffi::c_void;

#[link(name = "ScriptHookV", kind = "raw-dylib")]
extern "C" {
    #[link_name = "?scriptRegister@@YAXPEAUHINSTANCE__@@P6AXXZ@Z"]
    pub fn script_register(module: *mut c_void, main: extern "C" fn());
    #[link_name = "?scriptUnregister@@YAXPEAUHINSTANCE__@@@Z"]
    pub fn script_unregister(module: *mut c_void);
    #[link_name = "?scriptWait@@YAXK@Z"]
    fn script_wait(ms: u32);
    #[link_name = "?nativeInit@@YAX_K@Z"]
    fn native_init(hash: u64);
    #[link_name = "?nativePush64@@YAX_K@Z"]
    fn native_push64(value: u64);
    #[link_name = "?nativeCall@@YAPEA_KXZ"]
    fn native_call() -> *mut u64;
    #[link_name = "?worldGetAllObjects@@YAHPEAHH@Z"]
    fn world_get_all_objects(arr: *mut i32, size: i32) -> i32;
    #[link_name = "?worldGetAllPeds@@YAHPEAHH@Z"]
    fn world_get_all_peds(arr: *mut i32, size: i32) -> i32;
    #[link_name = "?worldGetAllVehicles@@YAHPEAHH@Z"]
    fn world_get_all_vehicles(arr: *mut i32, size: i32) -> i32;
}

/// Handles of every object (props and their broken pieces; up to 2048).
pub fn all_objects() -> Vec<i32> {
    let mut v = vec![0i32; 2048];
    let n = unsafe { world_get_all_objects(v.as_mut_ptr(), v.len() as i32) };
    v.truncate(n.clamp(0, 2048) as usize);
    v
}

/// Handles of every ped in the world (up to 1024).
pub fn all_peds() -> Vec<i32> {
    let mut v = vec![0i32; 1024];
    let n = unsafe { world_get_all_peds(v.as_mut_ptr(), v.len() as i32) };
    v.truncate(n.clamp(0, 1024) as usize);
    v
}

/// Handles of every vehicle in the world (up to 1024).
pub fn all_vehicles() -> Vec<i32> {
    let mut v = vec![0i32; 1024];
    let n = unsafe { world_get_all_vehicles(v.as_mut_ptr(), v.len() as i32) };
    v.truncate(n.clamp(0, 1024) as usize);
    v
}

/// Yields the script fiber back to the game for `ms` milliseconds.
pub fn wait(ms: u32) {
    unsafe { script_wait(ms) }
}

/// A value passed to a native. ScriptHookV takes every argument as a 64-bit
/// slot: integers and BOOLs zero-extended, floats as their bits in the low
/// half, pointers as addresses.
pub trait Arg {
    fn slot(self) -> u64;
}
impl Arg for i32 {
    fn slot(self) -> u64 {
        self as u32 as u64
    }
}
impl Arg for u32 {
    fn slot(self) -> u64 {
        self as u64
    }
}
impl Arg for f32 {
    fn slot(self) -> u64 {
        self.to_bits() as u64
    }
}
impl Arg for bool {
    fn slot(self) -> u64 {
        self as u64
    }
}
impl<T> Arg for *mut T {
    fn slot(self) -> u64 {
        self as usize as u64
    }
}
impl<T> Arg for *const T {
    fn slot(self) -> u64 {
        self as usize as u64
    }
}

/// Invokes a native by its original hash; returns the result slot pointer.
pub fn call(hash: u64, args: &[u64]) -> *mut u64 {
    unsafe {
        native_init(hash);
        for &a in args {
            native_push64(a);
        }
        native_call()
    }
}

/// Game-side Vector3: three floats, each padded to 8 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Vec3 {
    pub x: f32,
    _p0: u32,
    pub y: f32,
    _p1: u32,
    pub z: f32,
    _p2: u32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z, ..Default::default() }
    }
}
