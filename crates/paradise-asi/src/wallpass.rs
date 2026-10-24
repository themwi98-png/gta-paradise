//! Lets aircraft and boats through Burnout's invisible walls. The walls'
//! collision is typed MAP_STAIRS and includes only vehicles (bpx gta.rs);
//! every vehicle includes MAP_STAIRS but meets nothing else of that type.
//! Clearing MAP_STAIRS from an aircraft's or boat's include flags makes the
//! walls let it through, while the map (MAP_VEHICLE) still stops it.
//!
//! Where GTA keeps the flags it collides with (Ghidra on a memory dump of
//! GTA5.exe Legacy, 2026-10-24; see private/NOTES.md): the physics level, a
//! global pointer. Level + 0x50 is a table of 0x30-byte entries, one per
//! object, indexed by the u16 at phInst + 0x18. Entry + 0x10 is the include
//! flags, + 0x14 the type flags. GTA copies them from the archetype's + 0x2c
//! / + 0x28 (phInst + 0x10 is the archetype) and rewrites them now and then
//! (collision off and on, resets from the archetype). So both copies are
//! patched, and every vehicle is re-checked often. The entity's phInst is
//! entity + 0x30.
//!
//! The global is found from a fingerprint of game code that indexes the
//! table, not a fixed address. Every read and write goes through
//! Read/WriteProcessMemory on the game's own process, which fail instead of
//! crashing; nothing is written unless the layout checks out.

use crate::{log, natives as n};
use std::collections::HashSet;
use std::ffi::c_void;

const INST_AT: usize = 0x30;
const ARCHETYPE_AT: usize = 0x10;
const LEVEL_INDEX_AT: usize = 0x18;
const ARCH_TYPE_AT: usize = 0x28;
const ARCH_INCLUDE_AT: usize = 0x2c;
const LEVEL_ENTRIES_AT: usize = 0x50;
const ENTRY_SIZE: usize = 0x30;
const ENTRY_INCLUDE_AT: usize = 0x10;
const ENTRY_TYPE_AT: usize = 0x14;

const MAP_VEHICLE: u32 = 1 << 5;
const VEHICLE_TYPES: u32 = (1 << 6) | (1 << 7) | (1 << 8);
const MAP_STAIRS: u32 = 1 << 30;

/// Vehicle classes let through: boats, helicopters, planes.
const CLASSES: [i32; 3] = [14, 15, 16];
/// Every vehicle is re-checked this often (the player's every frame).
const SCAN_MS: i32 = 250;

/// mov rdx, [rip + level]; test rdx, rdx; jz ..; movzx eax, word [rax + 0x18];
/// lea rcx, [rax + rax*2]; mov rax, [rdx + 0x50]; add rcx, rcx
/// (-1: any byte). The level global is at the end of the first instruction
/// plus its 32-bit displacement.
const FINGERPRINT: [i16; 26] = [
    0x48, 0x8b, 0x15, -1, -1, -1, -1, 0x48, 0x85, 0xd2, 0x74, -1, 0x0f, 0xb7, 0x40, 0x18, 0x48, 0x8d, 0x0c, 0x40, 0x48, 0x8b, 0x42, 0x50, 0x48, 0x03,
];

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    #[link_name = "GetCurrentProcess"]
    fn get_current_process() -> *mut c_void;
    #[link_name = "ReadProcessMemory"]
    fn read_process_memory(process: *mut c_void, address: *const c_void, buffer: *mut c_void, size: usize, read: *mut usize) -> i32;
    #[link_name = "WriteProcessMemory"]
    fn write_process_memory(process: *mut c_void, address: *mut c_void, buffer: *const c_void, size: usize, written: *mut usize) -> i32;
    #[link_name = "GetModuleHandleW"]
    fn get_module_handle(name: *const u16) -> *mut c_void;
}

#[link(name = "ScriptHookV", kind = "raw-dylib")]
extern "C" {
    #[link_name = "?getScriptHandleBaseAddress@@YAPEAEH@Z"]
    fn get_script_handle_base_address(handle: i32) -> *mut u8;
    #[link_name = "?worldGetAllVehicles@@YAHPEAHH@Z"]
    fn world_get_all_vehicles(arr: *mut i32, size: i32) -> i32;
}

fn read(addr: usize, buf: &mut [u8]) -> bool {
    let mut got = 0usize;
    addr >= 0x10000 && unsafe { read_process_memory(get_current_process(), addr as *const c_void, buf.as_mut_ptr().cast(), buf.len(), &mut got) } != 0 && got == buf.len()
}

fn read_u32(addr: usize) -> Option<u32> {
    let mut b = [0u8; 4];
    read(addr, &mut b).then(|| u32::from_le_bytes(b))
}

fn read_ptr(addr: usize) -> Option<usize> {
    let mut b = [0u8; 8];
    read(addr, &mut b).then(|| u64::from_le_bytes(b) as usize).filter(|&p| p >= 0x10000)
}

fn write_u32(addr: usize, v: u32) -> bool {
    let b = v.to_le_bytes();
    let mut put = 0usize;
    unsafe { write_process_memory(get_current_process(), addr as *mut c_void, b.as_ptr().cast(), 4, &mut put) != 0 && put == 4 }
}

/// The address of the physics level global, from FINGERPRINT in GTA5.exe's
/// code (exactly one match).
fn find_level_global() -> Result<usize, String> {
    let base = unsafe { get_module_handle(std::ptr::null()) } as usize;
    let size = read_u32(base + 0x3c).and_then(|nt| read_u32(base + nt as usize + 0x18 + 0x38)).ok_or("no GTA5.exe image header")? as usize;
    let mut image = vec![0u8; size];
    for (k, page) in image.chunks_mut(0x1000).enumerate() {
        if !read(base + k * 0x1000, page) {
            page.fill(0);
        }
    }
    let hits: Vec<usize> = image
        .windows(FINGERPRINT.len())
        .enumerate()
        .filter(|(_, w)| w.iter().zip(&FINGERPRINT).all(|(&b, &f)| f < 0 || b == f as u8))
        .map(|(i, _)| i)
        .collect();
    if hits.len() != 1 {
        return Err(format!("the code fingerprint matched {} times (wanted once); a different GTA5.exe?", hits.len()));
    }
    let at = hits[0];
    let disp = i32::from_le_bytes([image[at + 3], image[at + 4], image[at + 5], image[at + 6]]);
    Ok((base as i64 + at as i64 + 7 + disp as i64) as usize)
}

enum Outcome {
    /// MAP_STAIRS already clear in the level entry (and archetype).
    Already,
    Patched { index: usize, ty: u32, before: u32, after: u32, archetype: bool },
    Skipped(String),
}

fn let_through(global: usize, vehicle: i32) -> Outcome {
    let entity = unsafe { get_script_handle_base_address(vehicle) } as usize;
    let Some(inst) = read_ptr(entity + INST_AT) else {
        return Outcome::Skipped(format!("entity {entity:#x} has no readable phInst"));
    };
    let Some(index) = read_u32(inst + LEVEL_INDEX_AT).map(|v| (v & 0xffff) as usize) else {
        return Outcome::Skipped("level index unreadable".into());
    };
    if index == 0xffff {
        return Outcome::Already; // not in the physics level right now
    }
    let Some(entry) = read_ptr(global).and_then(|level| read_ptr(level + LEVEL_ENTRIES_AT)).map(|e| e + index * ENTRY_SIZE) else {
        return Outcome::Skipped("physics level table unreadable".into());
    };
    let (Some(include), Some(ty)) = (read_u32(entry + ENTRY_INCLUDE_AT), read_u32(entry + ENTRY_TYPE_AT)) else {
        return Outcome::Skipped(format!("level entry {entry:#x} unreadable"));
    };
    // A vehicle's entry: a vehicle type bit. Its include may be anything GTA
    // set (0 or 0x230000 with collision off); only MAP_STAIRS is touched.
    if ty & VEHICLE_TYPES == 0 {
        return Outcome::Skipped(format!("level entry {entry:#x} (index {index}) doesn't look like a vehicle's: type {ty:#010x}, include {include:#010x}"));
    }
    // The archetype too, so GTA's resets from it keep the bit clear.
    let mut archetype = false;
    if let Some(arch) = read_ptr(inst + ARCHETYPE_AT) {
        if let (Some(at), Some(ai)) = (read_u32(arch + ARCH_TYPE_AT), read_u32(arch + ARCH_INCLUDE_AT)) {
            if at & VEHICLE_TYPES != 0 && ai & MAP_VEHICLE != 0 && ai & MAP_STAIRS != 0 {
                archetype = write_u32(arch + ARCH_INCLUDE_AT, ai & !MAP_STAIRS);
            }
        }
    }
    if include & MAP_STAIRS == 0 {
        return if archetype { Outcome::Patched { index, ty, before: include, after: include, archetype } } else { Outcome::Already };
    }
    let after = include & !MAP_STAIRS;
    if !write_u32(entry + ENTRY_INCLUDE_AT, after) {
        return Outcome::Skipped(format!("level entry {entry:#x} not writable"));
    }
    Outcome::Patched { index, ty, before: include, after, archetype }
}

fn all_vehicles() -> Vec<i32> {
    let mut v = vec![0i32; 1024];
    let n = unsafe { world_get_all_vehicles(v.as_mut_ptr(), v.len() as i32) };
    v.truncate(n.clamp(0, 1024) as usize);
    v
}

pub struct WallPass {
    /// The level global, or why it wasn't found (then nothing is patched).
    global: Option<Result<usize, String>>,
    last_scan: i32,
    /// (vehicle, level index) pairs already logged.
    logged: HashSet<(i32, usize)>,
    skipped: HashSet<i32>,
    patches: usize,
}

impl Default for WallPass {
    fn default() -> Self {
        Self { global: None, last_scan: 0, logged: HashSet::new(), skipped: HashSet::new(), patches: 0 }
    }
}

impl WallPass {
    /// `player_vehicle`: the vehicle the player is in (0 on foot).
    pub fn update(&mut self, player_vehicle: i32, now: i32) {
        let global = match &self.global {
            Some(Ok(g)) => *g,
            Some(Err(_)) => return,
            None => {
                let found = find_level_global();
                match &found {
                    Ok(g) => log(&format!("wallpass: physics level global at {g:#x} (level {:#x})", read_ptr(*g).unwrap_or(0))),
                    Err(why) => log(&format!("wallpass: physics level not found, aircraft and boats keep hitting the invisible walls: {why}")),
                }
                self.global = Some(found);
                return;
            }
        };
        if player_vehicle != 0 {
            self.check(global, player_vehicle);
        }
        if now - self.last_scan < SCAN_MS {
            return;
        }
        self.last_scan = now;
        for v in all_vehicles() {
            if v != player_vehicle {
                self.check(global, v);
            }
        }
        if self.logged.len() > 4096 {
            self.logged.clear();
        }
    }

    fn check(&mut self, global: usize, vehicle: i32) {
        let class = n::get_vehicle_class(vehicle);
        if !CLASSES.contains(&class) {
            return;
        }
        let name = || {
            let model = n::get_entity_model(vehicle);
            n::display_name(model).unwrap_or_else(|| format!("{model:#x}"))
        };
        match let_through(global, vehicle) {
            Outcome::Already => {}
            Outcome::Patched { index, ty, before, after, archetype } => {
                self.patches += 1;
                if self.logged.insert((vehicle, index)) {
                    log(&format!(
                        "wallpass: {} (class {class}, vehicle {vehicle}) passes the invisible walls: level index {index}, type {ty:#010x}, include {before:#010x} -> {after:#010x}{} ({} patches so far)",
                        name(),
                        if archetype { ", archetype too" } else { "" },
                        self.patches
                    ));
                }
            }
            Outcome::Skipped(why) => {
                if self.skipped.insert(vehicle) {
                    log(&format!("wallpass: {} (class {class}, vehicle {vehicle}) left alone: {why}", name()));
                }
            }
        }
    }
}
