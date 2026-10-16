//! Typed wrappers for the natives we use. Hashes are the original (2015)
//! names' hashes, which ScriptHookV translates for the running game version;
//! verified against alloc8or/gta5-nativedb-data.

use crate::shv::{call, Arg, Vec3};
use std::ffi::CString;

macro_rules! args {
    ($($a:expr),*) => { &[$(Arg::slot($a)),*] };
}

fn ret_i32(p: *mut u64) -> i32 {
    unsafe { *(p as *const i32) }
}
fn ret_bool(p: *mut u64) -> bool {
    ret_i32(p) != 0
}
fn ret_f32(p: *mut u64) -> f32 {
    unsafe { *(p as *const f32) }
}

pub fn player_ped_id() -> i32 {
    ret_i32(call(0xD80958FC74E988A6, &[]))
}

pub fn get_entity_coords(entity: i32) -> Vec3 {
    unsafe { *(call(0x3FEF770D40960D5A, args!(entity, true)) as *const Vec3) }
}

pub fn get_entity_heading(entity: i32) -> f32 {
    ret_f32(call(0xE83D4F9BA2A38914, args!(entity)))
}

pub fn set_entity_coords(entity: i32, p: Vec3) {
    call(0x06843DA7060A026B, args!(entity, p.x, p.y, p.z, false, false, false, false));
}

pub fn freeze_entity_position(entity: i32, frozen: bool) {
    call(0x428CA6DBD1094446, args!(entity, frozen));
}

pub fn set_entity_velocity(entity: i32, x: f32, y: f32, z: f32) {
    call(0x1C99BB7B6E96D16F, args!(entity, x, y, z));
}

pub fn is_entity_in_air(entity: i32) -> bool {
    ret_bool(call(0x886E37EC497200B6, args!(entity)))
}

pub fn request_collision_at_coord(p: Vec3) {
    call(0x07503F7948F491A7, args!(p.x, p.y, p.z));
}

pub fn has_collision_loaded_around_entity(entity: i32) -> bool {
    ret_bool(call(0xE9676F61BC0B3321, args!(entity)))
}

pub fn is_ped_in_any_vehicle(ped: i32) -> bool {
    ret_bool(call(0x997ABD671D25CA0B, args!(ped, false)))
}

pub fn get_vehicle_ped_is_in(ped: i32) -> i32 {
    ret_i32(call(0x9A9112A0FE9A4713, args!(ped, false)))
}

pub fn get_game_timer() -> i32 {
    ret_i32(call(0x9CD27B0045628463, &[]))
}

pub fn get_ground_z(p: Vec3) -> Option<f32> {
    let mut z = 0f32;
    let ok = ret_bool(call(0xC906A7DAB05C8D2B, args!(p.x, p.y, p.z, &mut z as *mut f32, false, false)));
    ok.then_some(z)
}

#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub pos: Vec3,
    pub normal: Vec3,
    pub material: u32,
    pub entity: i32,
}

/// Synchronous line test. `flags`: 1 map/mover, 2 vehicles, 4 peds,
/// 16 objects, 256 foliage, 511 everything.
pub fn raycast(from: Vec3, to: Vec3, flags: i32, ignore: i32) -> Option<RayHit> {
    let handle = ret_i32(call(
        0x377906D8A31E5586,
        args!(from.x, from.y, from.z, to.x, to.y, to.z, flags, ignore, 7),
    ));
    let mut hit = 0i32;
    let mut pos = Vec3::default();
    let mut normal = Vec3::default();
    let mut material = 0u32;
    let mut entity = 0i32;
    call(
        0x65287525D951F6BE,
        args!(
            handle,
            &mut hit as *mut i32,
            &mut pos as *mut Vec3,
            &mut normal as *mut Vec3,
            &mut material as *mut u32,
            &mut entity as *mut i32
        ),
    );
    (hit != 0).then_some(RayHit { pos, normal, material, entity })
}

/// Draws one line of text in screen space (0..1); `scale` ~0.35 is small.
pub fn draw_text(text: &str, x: f32, y: f32, scale: f32, rgba: [i32; 4]) {
    let entry = CString::new("STRING").unwrap();
    let body = CString::new(text.replace('\0', "")).unwrap();
    call(0x66E0276CC5F6B9DA, args!(4)); // SET_TEXT_FONT (4 = condensed)
    call(0x07C837F9A01C34C9, args!(scale, scale)); // SET_TEXT_SCALE
    call(0xBE6B23FFA53FB442, args!(rgba[0], rgba[1], rgba[2], rgba[3])); // SET_TEXT_COLOUR
    call(0x2513DFB0FB8400FE, &[]); // SET_TEXT_OUTLINE
    call(0x25FBB336DF1804CB, args!(entry.as_ptr())); // BEGIN_TEXT_COMMAND_DISPLAY_TEXT
    call(0x6C188BE134E074AA, args!(body.as_ptr())); // ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME
    call(0xCD015E5BB0D96A57, args!(x, y, 0)); // END_TEXT_COMMAND_DISPLAY_TEXT
}

/// Applies story-mode map DLC change sets (GROUP_MAP_SP) and unloads Online
/// ones. Not done by the game by default in story mode.
pub fn on_enter_sp() {
    call(0xD7C10C4A637992C9, &[]);
}

pub fn is_loading_screen_active() -> bool {
    ret_bool(call(0x10D0A8F259E93EC9, &[]))
}

/// Draws a one-sided, depth-tested triangle in world space this frame.
pub fn draw_poly(a: [f32; 3], b: [f32; 3], c: [f32; 3], rgba: [i32; 4]) {
    call(
        0xAC26716048436851,
        args!(a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2], rgba[0], rgba[1], rgba[2], rgba[3]),
    );
}

pub fn get_entity_velocity(entity: i32) -> Vec3 {
    unsafe { *(call(0x4805D2B1D8CF94A9, args!(entity)) as *const Vec3) }
}

/// Speed in m/s.
pub fn get_entity_speed(entity: i32) -> f32 {
    ret_f32(call(0xD5037BA82E12416F, args!(entity)))
}

/// IS_ENTITY_DEAD
pub fn is_entity_dead(entity: i32) -> bool {
    ret_bool(call(0x5F9532F3B5CC2551, args!(entity, false)))
}

/// IS_PED_IN_FLYING_VEHICLE
pub fn is_ped_in_flying_vehicle(ped: i32) -> bool {
    ret_bool(call(0x9134873537FA419C, args!(ped)))
}

/// IS_ENTITY_IN_WATER
pub fn is_entity_in_water(entity: i32) -> bool {
    ret_bool(call(0xCFB0A0D8EDD145A3, args!(entity)))
}

pub fn set_random_trains(on: bool) {
    call(0x80D9F74197EA47D9, args!(on));
}

pub fn delete_all_trains() {
    call(0x736A718577F39C7D, &[]);
}

pub fn switch_train_track(track: i32, on: bool) {
    call(0xFD813BB7DB977F20, args!(track, on));
}

pub fn is_vehicle_on_all_wheels(vehicle: i32) -> bool {
    ret_bool(call(0xB104CD1BABF302E2, args!(vehicle)))
}

pub fn does_entity_exist(entity: i32) -> bool {
    ret_bool(call(0x7239B21A38F536BA, args!(entity)))
}

/// Removes the vehicles (not the player's) within `radius` of `p`.
pub fn clear_area_of_vehicles(p: Vec3, radius: f32) {
    call(0x01C7B9B38428AEB6, args!(p.x, p.y, p.z, radius, false, false, false, false, false, false, 0));
}

/// Removes the NPCs within `radius` of `p`.
pub fn clear_area_of_peds(p: Vec3, radius: f32) {
    call(0xBE31FD6CE464AC59, args!(p.x, p.y, p.z, radius, 1));
}

pub fn stop_fire_in_range(p: Vec3, radius: f32) {
    call(0x056A8A219B8E829F, args!(p.x, p.y, p.z, radius));
}

// ---- Vehicles, crashes and respawning ----

pub fn player_id() -> i32 {
    ret_i32(call(0x4F8644AF03D0E0D6, &[]))
}

/// IS_PLAYER_BEING_ARRESTED
pub fn is_player_being_arrested(player: i32) -> bool {
    ret_bool(call(0x388A47C51ABDAC8E, args!(player, true)))
}

pub fn get_entity_model(entity: i32) -> u32 {
    ret_i32(call(0x9F47B058362C84B5, args!(entity))) as u32
}

/// GET_VEHICLE_CLASS: 14 boats, 15 helicopters, 16 planes, 21 trains.
pub fn get_vehicle_class(vehicle: i32) -> i32 {
    ret_i32(call(0x29439776AAA00A62, args!(vehicle)))
}

/// GET_PED_IN_VEHICLE_SEAT (-1 = driver)
pub fn get_ped_in_vehicle_seat(vehicle: i32, seat: i32) -> i32 {
    ret_i32(call(0xBB40DD2270B65366, args!(vehicle, seat, false)))
}

pub fn get_entity_forward_vector(entity: i32) -> Vec3 {
    unsafe { *(call(0x0A794A5A57F8DF91, args!(entity)) as *const Vec3) }
}

/// SET_VEHICLE_DAMAGE: dents the body around a point given relative to the
/// vehicle (x right, y forward, z up).
pub fn set_vehicle_damage(vehicle: i32, x: f32, y: f32, z: f32, damage: f32, radius: f32) {
    call(0xA1DD317EA8FD4F29, args!(vehicle, x, y, z, damage, radius, true));
}

pub fn smash_vehicle_window(vehicle: i32, index: i32) {
    call(0x9E5B5E4D2CCD2259, args!(vehicle, index));
}

/// SET_VEHICLE_DOOR_BROKEN: 0-3 doors, 4 bonnet, 5 boot; `delete` removes
/// it instead of dropping it.
pub fn set_vehicle_door_broken(vehicle: i32, door: i32, delete: bool) {
    call(0xD4D4F6A4AB575A33, args!(vehicle, door, delete));
}

pub fn set_vehicle_tyre_burst(vehicle: i32, index: i32) {
    call(0xEC6A202EE4960385, args!(vehicle, index, true, 1000.0f32));
}

pub fn set_vehicle_engine_health(vehicle: i32, health: f32) {
    call(0x45F6D8EEF34ABEF1, args!(vehicle, health));
}

pub fn set_vehicle_petrol_tank_health(vehicle: i32, health: f32) {
    call(0x70DB57649FA8D0D8, args!(vehicle, health));
}

/// EXPLODE_VEHICLE
pub fn explode_vehicle(vehicle: i32) {
    call(0xBA71116ADF5B514C, args!(vehicle, true, false));
}

/// ADD_EXPLOSION (type 7 = car).
pub fn add_explosion(p: Vec3, kind: i32, damage_scale: f32, shake: f32) {
    call(0xE3AD2BDBAEE269AC, args!(p.x, p.y, p.z, kind, damage_scale, true, false, shake, false));
}

pub fn set_entity_invincible(entity: i32, on: bool) {
    call(0x3882114BDE571AD4, args!(entity, on, false));
}

pub fn set_player_invincible(player: i32, on: bool) {
    call(0x239528EACDC3E7DE, args!(player, on));
}

pub fn set_entity_health(entity: i32, health: i32) {
    call(0x6B76DC1F3AE6E6A3, args!(entity, health, 0, 0u32));
}

pub fn get_entity_max_health(entity: i32) -> i32 {
    ret_i32(call(0x15D757606D170C3C, args!(entity)))
}

/// SET_PED_CONFIG_FLAG (32 = can fly through the windscreen).
pub fn set_ped_config_flag(ped: i32, flag: i32, value: bool) {
    call(0x1913FE4CBF41C463, args!(ped, flag, value));
}

pub fn set_time_scale(scale: f32) {
    call(0x1D408577D440E81E, args!(scale));
}

pub fn shake_gameplay_cam(name: &str, amplitude: f32) {
    let s = CString::new(name).unwrap();
    call(0xFD55E49555E017CF, args!(s.as_ptr(), amplitude));
}

pub fn do_screen_fade_out(ms: i32) {
    call(0x891B5B39AC6302AF, args!(ms));
}

pub fn do_screen_fade_in(ms: i32) {
    call(0xD4E8E24955024033, args!(ms));
}

pub fn is_screen_faded_out() -> bool {
    ret_bool(call(0xB16FCE9DDC7BA182, &[]))
}

/// GET_CLOSEST_VEHICLE_NODE_WITH_HEADING: nearest road node and its heading.
pub fn closest_vehicle_node(p: Vec3) -> Option<(Vec3, f32)> {
    let mut out = Vec3::default();
    let mut heading = 0f32;
    let ok = ret_bool(call(
        0xFF071FB798B803B0,
        args!(p.x, p.y, p.z, &mut out as *mut Vec3, &mut heading as *mut f32, 1, 3.0f32, 0),
    ));
    ok.then_some((out, heading))
}

/// GET_SAFE_COORD_FOR_PED: a spot on the pedestrian navmesh near `p`.
pub fn safe_coord_for_ped(p: Vec3) -> Option<Vec3> {
    let mut out = Vec3::default();
    let ok = ret_bool(call(0xB61C8E878A4199CA, args!(p.x, p.y, p.z, true, &mut out as *mut Vec3, 16)));
    ok.then_some(out)
}

pub fn clear_ped_tasks_immediately(ped: i32) {
    call(0xAAA34F8A7CB32098, args!(ped));
}

/// Deletes a vehicle (made a mission entity first so the game lets go of it).
pub fn delete_vehicle(vehicle: i32) {
    call(0xAD738C3085FE7E11, args!(vehicle, true, true));
    let mut v = vehicle;
    call(0xEA386986E786A54F, args!(&mut v as *mut i32));
}

/// Deletes a ped (made a mission entity first so the game lets go of it).
pub fn delete_ped(ped: i32) {
    call(0xAD738C3085FE7E11, args!(ped, true, true));
    let mut p = ped;
    call(0x9614299DCB53E54B, args!(&mut p as *mut i32));
}

pub fn set_vehicle_as_no_longer_needed(vehicle: i32) {
    let mut v = vehicle;
    call(0x629BFA74418D6239, args!(&mut v as *mut i32));
}

pub fn request_model(model: u32) {
    call(0x963D27A58DF860AC, args!(model));
}

pub fn has_model_loaded(model: u32) -> bool {
    ret_bool(call(0x98A4EB5D89A0C952, args!(model)))
}

pub fn set_model_as_no_longer_needed(model: u32) {
    call(0xE532F5D78798DAAB, args!(model));
}

pub fn create_vehicle(model: u32, p: Vec3, heading: f32) -> i32 {
    ret_i32(call(0xAF35D0D2583051B0, args!(model, p.x, p.y, p.z, heading, false, false, false)))
}

pub fn set_ped_into_vehicle(ped: i32, vehicle: i32) {
    call(0xF75B0D629E1C063D, args!(ped, vehicle, -1));
}

pub fn set_vehicle_on_ground_properly(vehicle: i32) {
    call(0x49733E92263139D1, args!(vehicle, 5.0f32));
}

pub fn set_vehicle_engine_on(vehicle: i32) {
    call(0x2497C4717C8B881E, args!(vehicle, true, true, false));
}

pub fn set_vehicle_forward_speed(vehicle: i32, speed: f32) {
    call(0xAB54A438726D25D5, args!(vehicle, speed));
}

pub fn set_entity_heading(entity: i32, heading: f32) {
    call(0x8E2530AA8ADA980E, args!(entity, heading));
}

/// A vehicle's look (colours, livery, upgrades), to rebuild it after a wreck.
#[derive(Clone, Default)]
pub struct VehicleLook {
    colours: [i32; 4],
    livery: i32,
    wheel_type: i32,
    tint: i32,
    mods: Vec<(i32, i32, bool)>,
    toggles: Vec<i32>,
    plate: Option<CString>,
}

pub fn vehicle_look(v: i32) -> VehicleLook {
    let mut l = VehicleLook::default();
    let [a, b, c, d] = &mut l.colours;
    call(0xA19435F193E081AC, args!(v, a as *mut i32, b as *mut i32));
    call(0x3BC4245933A166F7, args!(v, c as *mut i32, d as *mut i32));
    l.livery = ret_i32(call(0x2BB9230590DA5E8A, args!(v)));
    l.wheel_type = ret_i32(call(0xB3ED1BFB4BE636DC, args!(v)));
    l.tint = ret_i32(call(0x0EE21293DAD47C95, args!(v)));
    for m in 0..50 {
        if (17..=22).contains(&m) {
            if ret_bool(call(0x84B233A8C8FC8AE7, args!(v, m))) {
                l.toggles.push(m);
            }
            continue;
        }
        let i = ret_i32(call(0x772960298DA26FDB, args!(v, m)));
        if i >= 0 {
            l.mods.push((m, i, ret_bool(call(0xB3924ECD70E095DC, args!(v, m)))));
        }
    }
    let plate = unsafe { *(call(0x7CE1CCB9B293020E, args!(v)) as *const *const std::ffi::c_char) };
    if !plate.is_null() {
        l.plate = Some(unsafe { std::ffi::CStr::from_ptr(plate) }.to_owned());
    }
    l
}

pub fn apply_vehicle_look(v: i32, l: &VehicleLook) {
    call(0x1F2AA07F00B3217A, args!(v, 0)); // SET_VEHICLE_MOD_KIT
    call(0x487EB21CC7295BA1, args!(v, l.wheel_type));
    for &(m, i, custom) in &l.mods {
        call(0x6AF0636DDEDCB6DD, args!(v, m, i, custom));
    }
    for &m in &l.toggles {
        call(0x2A1F4F37F95BAD08, args!(v, m, true));
    }
    call(0x4F1D4BE3A7F24601, args!(v, l.colours[0], l.colours[1]));
    call(0x2036F561ADD12E33, args!(v, l.colours[2], l.colours[3]));
    if l.livery >= 0 {
        call(0x60BF608F1B8CD1B6, args!(v, l.livery));
    }
    if l.tint >= 0 {
        call(0x57C51E6BAD752696, args!(v, l.tint));
    }
    if let Some(p) = &l.plate {
        call(0x95A88F0B409CDA47, args!(v, p.as_ptr()));
    }
}

// ---- Scripted cameras ----

pub fn create_cam() -> i32 {
    let name = CString::new("DEFAULT_SCRIPTED_CAMERA").unwrap();
    ret_i32(call(0xC3981DCE61D9E13F, args!(name.as_ptr(), false)))
}

pub fn set_cam_coord(cam: i32, p: Vec3) {
    call(0x4D41783FB745E42E, args!(cam, p.x, p.y, p.z));
}

pub fn point_cam_at_entity(cam: i32, entity: i32) {
    call(0x5640BFF86B16E8DC, args!(cam, entity, 0.0f32, 0.0f32, 0.0f32, true));
}

pub fn set_cam_fov(cam: i32, fov: f32) {
    call(0xB13C14F66A00D047, args!(cam, fov));
}

pub fn set_cam_active(cam: i32, on: bool) {
    call(0x026FB97D0A425F84, args!(cam, on));
}

/// RENDER_SCRIPT_CAMS: switch between script cameras and the gameplay camera.
pub fn render_script_cams(on: bool, ease_ms: i32) {
    call(0x07E5B515DB0636FC, args!(on, ease_ms > 0, ease_ms, true, false, 0));
}

pub fn destroy_cam(cam: i32) {
    call(0x865908C81A2C22E9, args!(cam, false));
}

// ---- Boost ----

/// DISABLE_CONTROL_ACTION (pad 0) for this frame.
pub fn disable_control(action: i32) {
    call(0xFE99B66D079CF6BC, args!(0, action, true));
}

/// IS_DISABLED_CONTROL_PRESSED (pad 0)
pub fn is_disabled_control_pressed(action: i32) -> bool {
    ret_bool(call(0xE2587F8CBBD87B1D, args!(0, action)))
}

/// Seconds since the last frame (game time).
pub fn get_frame_time() -> f32 {
    ret_f32(call(0x15C40837039FFAF7, &[]))
}

/// GET_VEHICLE_ESTIMATED_MAX_SPEED (m/s)
pub fn get_vehicle_estimated_max_speed(vehicle: i32) -> f32 {
    ret_f32(call(0x53AF99BAA671CA47, args!(vehicle)))
}

/// Velocity in the entity's own frame (x right, y forward, z up).
pub fn get_entity_speed_vector_relative(entity: i32) -> Vec3 {
    unsafe { *(call(0x9A8D700A51CB7B0D, args!(entity, true)) as *const Vec3) }
}

pub fn has_entity_collided_with_anything(entity: i32) -> bool {
    ret_bool(call(0x8BAD02F0368D9E14, args!(entity)))
}

pub fn get_entity_bone_index_by_name(entity: i32, bone: &str) -> i32 {
    let b = CString::new(bone).unwrap();
    ret_i32(call(0xFB71170B7E76ACBA, args!(entity, b.as_ptr())))
}

pub fn request_ptfx_asset(asset: &str) -> bool {
    let a = CString::new(asset).unwrap();
    call(0xB80D8756B4668AB6, args!(a.as_ptr()));
    ret_bool(call(0x8702416E512EC454, args!(a.as_ptr())))
}

/// One-shot particle effect on an entity bone (from `asset`).
pub fn ptfx_on_bone(asset: &str, effect: &str, entity: i32, bone: i32, scale: f32) {
    let a = CString::new(asset).unwrap();
    let e = CString::new(effect).unwrap();
    call(0x6C38AF3693A69A91, args!(a.as_ptr()));
    call(
        0x02B1F2A72E0F5325,
        args!(e.as_ptr(), entity, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, bone, scale, false, false, false),
    );
}

/// Screen-space rectangle (centre x, y; size w, h; all 0..1).
pub fn draw_rect(x: f32, y: f32, w: f32, h: f32, rgba: [i32; 4]) {
    call(0x3A618A217E5154F0, args!(x, y, w, h, rgba[0], rgba[1], rgba[2], rgba[3], false));
}

/// SET_VEHICLE_CHEAT_POWER_INCREASE: engine torque multiplier, every frame.
pub fn set_vehicle_cheat_power_increase(vehicle: i32, value: f32) {
    call(0xB59E4BD37AE292DB, args!(vehicle, value));
}

/// SET_VEHICLE_BOOST_ACTIVE: GTA's speed-boost sound.
pub fn set_vehicle_boost_active(vehicle: i32, on: bool) {
    call(0x4A04DE7CAB2739A1, args!(vehicle, on));
}

/// GET_CLOSEST_ROAD: the nearest road segment (two node positions, from
/// `a` to `b`) and its lane counts along (a -> b) and against it.
pub fn closest_road(p: Vec3) -> Option<(Vec3, Vec3, i32, i32)> {
    let (mut a, mut b) = (Vec3::default(), Vec3::default());
    let (mut fwd, mut back, mut width) = (0i32, 0i32, 0f32);
    let ok = ret_bool(call(
        0x132F52BBA570FE92,
        args!(
            p.x,
            p.y,
            p.z,
            1.0f32,
            1,
            &mut a as *mut Vec3,
            &mut b as *mut Vec3,
            &mut fwd as *mut i32,
            &mut back as *mut i32,
            &mut width as *mut f32,
            false
        ),
    ));
    ok.then_some((a, b, fwd, back))
}

/// SET_VEHICLE_CAN_DEFORM_WHEELS
pub fn set_vehicle_can_deform_wheels(vehicle: i32, on: bool) {
    call(0x0CDDA42F9E360CA6, args!(vehicle, on));
}

/// SET_VEHICLE_HAS_STRONG_AXLES (false = axles bend)
pub fn set_vehicle_has_strong_axles(vehicle: i32, on: bool) {
    call(0x92F0CF722BC4202F, args!(vehicle, on));
}

/// SET_VEHICLE_WHEELS_CAN_BREAK_OFF_WHEN_BLOW_UP
pub fn set_vehicle_wheels_can_break_off(vehicle: i32, on: bool) {
    call(0xA37B9A517B133349, args!(vehicle, on));
}

// ---- Police chase diagnostics ----

pub fn get_player_wanted_level(player: i32) -> i32 {
    ret_i32(call(0xE28E54788CE8F12D, args!(player)))
}

/// GET_PED_TYPE: 6 cop, 27 SWAT, 29 army.
pub fn get_ped_type(ped: i32) -> i32 {
    ret_i32(call(0xFF059E1E4C01E63C, args!(ped)))
}

/// IS_VEHICLE_STUCK_TIMER_UP: 0 on roof, 1 on side, 2 hung up, 3 jammed.
pub fn is_vehicle_stuck_timer_up(vehicle: i32, kind: i32, ms: i32) -> bool {
    ret_bool(call(0x679BE1DAF71DA874, args!(vehicle, kind, ms)))
}

/// SET_DRIVER_ABILITY (0..1)
pub fn set_driver_ability(ped: i32, v: f32) {
    call(0xB195FFA8042FC5C3, args!(ped, v));
}

/// SET_DRIVER_AGGRESSIVENESS (0..1)
pub fn set_driver_aggressiveness(ped: i32, v: f32) {
    call(0xA731F608CA104E3C, args!(ped, v));
}

/// SET_DRIVER_RACING_MODIFIER (0..1)
pub fn set_driver_racing_modifier(ped: i32, v: f32) {
    call(0xDED5AF5A0EA4B297, args!(ped, v));
}

/// MODIFY_VEHICLE_TOP_SPEED: percentage added to the top speed.
pub fn modify_vehicle_top_speed(vehicle: i32, percent: f32) {
    call(0x93A3996368C94158, args!(vehicle, percent));
}

pub fn is_ped_a_player(ped: i32) -> bool {
    ret_bool(call(0x12534C348C6CB68B, args!(ped)))
}

/// LOAD_ALL_PATH_NODES: keep every road node on the map loaded (true), not
/// just those near the player. Returns whether all are loaded.
pub fn load_all_path_nodes(on: bool) -> bool {
    ret_bool(call(0xC2AB6BFE34E92F8B, args!(on)))
}

pub fn are_nodes_loaded_for_area(x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    ret_bool(call(0xF7B79A50B905A30D, args!(x1, y1, x2, y2)))
}

/// HAS_ENTITY_CLEAR_LOS_TO_ENTITY (trace type 17, as Rockstar's scripts).
pub fn has_clear_los(a: i32, b: i32) -> bool {
    ret_bool(call(0xFCDFF7B72D23A1AC, args!(a, b, 17)))
}

/// REPORT_POLICE_SPOTTED_PLAYER
pub fn report_police_spotted_player(player: i32) {
    call(0xDC64D2C53493ED12, args!(player));
}

pub fn set_player_wanted_centre(player: i32, p: Vec3) {
    call(0x520E541A97A13354, args!(player, p.x, p.y, p.z));
}

/// SET_PED_COMBAT_ATTRIBUTES (3 = may leave its vehicle).
/// TASK_VEHICLE_DRIVE_TO_COORD: drive to `p`, by road until within
/// `straight` m, then straight at it.
pub fn task_vehicle_drive_to_coord(driver: i32, vehicle: i32, p: Vec3, speed: f32, style: i32, stop: f32, straight: f32) {
    let model = get_entity_model(vehicle);
    call(0xE2A2AA2F659D77A7, args!(driver, vehicle, p.x, p.y, p.z, speed, 0, model, style, stop, straight));
}

/// TASK_VEHICLE_CHASE: GTA's own pursuit of `target`.
pub fn task_vehicle_chase(driver: i32, target: i32) {
    call(0x3C08A8E30363B353, args!(driver, target));
}

pub fn set_ped_combat_attribute(ped: i32, attr: i32, on: bool) {
    call(0x9F7794730795E019, args!(ped, attr, on));
}

pub fn set_ped_seeing_range(ped: i32, range: f32) {
    call(0xF29CF591C4BF6CEE, args!(ped, range));
}

// ---- Startup and damage ----









pub fn is_player_switch_in_progress() -> bool {
    ret_bool(call(0xD9D2CFFF49FAB35F, &[]))
}

pub fn stop_player_switch() {
    call(0x95C0A5BBDC189AA1, &[]);
}

pub fn is_entity_on_fire(entity: i32) -> bool {
    ret_bool(call(0x28D3FED7190D3A0B, args!(entity)))
}

pub fn is_vehicle_driveable(vehicle: i32) -> bool {
    ret_bool(call(0x4C241E39B23DF959, args!(vehicle, false)))
}

pub fn get_vehicle_engine_health(vehicle: i32) -> f32 {
    ret_f32(call(0xC45D23BAF168AAB8, args!(vehicle)))
}

pub fn get_vehicle_body_health(vehicle: i32) -> f32 {
    ret_f32(call(0xF271147EB7B40F12, args!(vehicle)))
}

pub fn set_vehicle_body_health(vehicle: i32, v: f32) {
    call(0xB77D05AC8C78AADB, args!(vehicle, v));
}


/// GET_CONTROL_NORMAL (pad 0), e.g. 71 = accelerate.
pub fn get_control_normal(action: i32) -> f32 {
    ret_f32(call(0xEC3C9B8D5327B563, args!(0, action)))
}

/// SET_VEHICLE_STRONG: no crash damage (bullets and explosions still hurt).
pub fn set_vehicle_strong(vehicle: i32, on: bool) {
    call(0x3E8C8727991A8A0B, args!(vehicle, on));
}

pub fn set_vehicle_can_be_visibly_damaged(vehicle: i32, on: bool) {
    call(0x4C7028F78FFD3681, args!(vehicle, on));
}


/// GET_CLOSEST_OBJECT_OF_TYPE (0 if none).
pub fn get_closest_object_of_type(p: Vec3, radius: f32, model: u32) -> i32 {
    ret_i32(call(0xE143FA2249364369, args!(p.x, p.y, p.z, radius, model, false, false, false)))
}

pub fn is_entity_touching_entity(a: i32, b: i32) -> bool {
    ret_bool(call(0x17FFC1B2BA35A494, args!(a, b)))
}

pub fn get_vehicle_petrol_tank_health(vehicle: i32) -> f32 {
    ret_f32(call(0x7D5DABE888D2D074, args!(vehicle)))
}

pub fn stop_entity_fire(entity: i32) {
    call(0x7F0DD2EBBB651AFF, args!(entity));
}

pub fn is_entity_a_vehicle(entity: i32) -> bool {
    ret_bool(call(0x6AC7003FA6E5575E, args!(entity)))
}

// ---- Spawn guard ----

/// GET_ENTITY_POPULATION_TYPE (2 random parked, 4 scenario, 5 ambient, 7 mission...).
pub fn get_entity_population_type(entity: i32) -> i32 {
    ret_i32(call(0xF6F5161F4534EDFF, args!(entity)))
}

/// GET_ENTITY_SCRIPT: the script that created the entity, if any.
pub fn get_entity_script(entity: i32) -> Option<String> {
    let p = unsafe { *(call(0xA6E9C38DB51D7748, args!(entity, std::ptr::null_mut::<i32>())) as *const *const std::ffi::c_char) };
    (!p.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

pub fn get_is_vehicle_engine_running(vehicle: i32) -> bool {
    ret_bool(call(0xAE31E7DF9B5B132E, args!(vehicle)))
}

/// ADD_SCENARIO_BLOCKING_AREA: no scenario peds / vehicles in this box.
pub fn add_scenario_blocking_area(a: Vec3, b: Vec3) {
    call(0x1B5C85C612E5256E, args!(a.x, a.y, a.z, b.x, b.y, b.z, false, true, true, true, 0));
}

/// Switches the parked-car generators in a box off (or on).
pub fn set_vehicle_generators_active_in_area(a: Vec3, b: Vec3, on: bool) {
    call(0xC12321827687FE4D, args!(a.x, a.y, a.z, b.x, b.y, b.z, on, true));
}

pub fn remove_vehicles_from_generators_in_area(a: Vec3, b: Vec3) {
    call(0x46A1E1A299EC4BBA, args!(a.x, a.y, a.z, b.x, b.y, b.z, 0));
}

// --- Clock ---------------------------------------------------------------

/// The in-game date and time: [hour, minute, second, day, month, year].
pub fn clock() -> [i32; 6] {
    [
        ret_i32(call(0x25223CA6B4D20B7F, &[])),
        ret_i32(call(0x13D2B8ADD79640F2, &[])),
        ret_i32(call(0x494E97C2EF27C470, &[])),
        ret_i32(call(0x3D10BC92A4DB1D35, &[])),
        ret_i32(call(0xBBC72712E80257A1, &[])),
        ret_i32(call(0x961777E64BDAF717, &[])),
    ]
}

pub fn set_clock(c: [i32; 6]) {
    call(0xB096419DF0D06CE7, args!(c[3], c[4], c[5]));
    call(0x47C3B5848C3E45D8, args!(c[0], c[1], c[2]));
}

// --- Driver protection -----------------------------------------------------

/// SET_ENTITY_PROOFS with only collision damage blocked (or nothing).
pub fn set_collision_proof(entity: i32, on: bool) {
    call(0xFAEE099C6F890BB8, args!(entity, false, false, false, on, false, false, false, false));
}

/// SET_PED_CAN_BE_KNOCKED_OFF_VEHICLE: 0 default, 1 never.
pub fn set_ped_can_be_knocked_off_vehicle(ped: i32, state: i32) {
    call(0x7A6535691B477C48, args!(ped, state));
}

// --- Big messages and frontend sounds -----------------------------------

pub fn request_scaleform_movie(name: &str) -> i32 {
    let s = CString::new(name).unwrap();
    ret_i32(call(0x11FE353CF9733E6F, args!(s.as_ptr())))
}

pub fn has_scaleform_movie_loaded(handle: i32) -> bool {
    ret_bool(call(0x85F01B8D5B90570E, args!(handle)))
}

/// Calls a method of a scaleform movie with string arguments.
pub fn scaleform_call(handle: i32, method: &str, strings: &[&str]) {
    let m = CString::new(method).unwrap();
    if !ret_bool(call(0xF6E48914C7A8694E, args!(handle, m.as_ptr()))) {
        return;
    }
    for s in strings {
        let c = CString::new(*s).unwrap();
        call(0xBA7148484BD90365, args!(c.as_ptr()));
    }
    call(0xC6796A8FFA375E53, &[]);
}

pub fn draw_scaleform_fullscreen(handle: i32) {
    call(0x0DF606929C105BE1, args!(handle, 255, 255, 255, 255, 0));
}

pub fn play_sound_frontend(name: &str, set: &str) {
    let (a, b) = (CString::new(name).unwrap(), CString::new(set).unwrap());
    call(0x67C540AA08E4A6F5, args!(-1, a.as_ptr(), b.as_ptr(), true));
}

/// SET_TIMECYCLE_MODIFIER at a strength (0..1).
pub fn set_timecycle_modifier(name: &str, strength: f32) {
    let c = CString::new(name).unwrap();
    call(0x2C933ABF17A1DF41, args!(c.as_ptr()));
    call(0x82E7FFCD5B2326B3, args!(strength));
}

pub fn clear_timecycle_modifier() {
    call(0x0F07E7745A236711, &[]);
}

pub fn shake_cam(cam: i32, name: &str, amplitude: f32) {
    let s = CString::new(name).unwrap();
    call(0x6A25241C340D3822, args!(cam, s.as_ptr(), amplitude));
}

/// No petrol tank damage or fires, and no engine fires.
pub fn set_disable_vehicle_fires(vehicle: i32, on: bool) {
    call(0x37C8252A7C92D017, args!(vehicle, on));
    call(0x465BF26AB9684352, args!(vehicle, on));
    call(0x91A0BD635321F145, args!(vehicle, on));
}

/// GET_ENTITY_ROTATION_VELOCITY: angular velocity (rad/s; z = yaw, + left).
pub fn get_entity_rotation_velocity(entity: i32) -> Vec3 {
    unsafe { *(call(0x213B91045D09B983, args!(entity)) as *const Vec3) }
}

pub fn set_entity_angular_velocity(entity: i32, x: f32, y: f32, z: f32) {
    call(0x8339643499D1222E, args!(entity, x, y, z));
}

/// DISABLE_CONTROL_ACTION (pad 0) for this frame, that action only.
pub fn disable_control_only(action: i32) {
    call(0xFE99B66D079CF6BC, args!(0, action, false));
}

/// GET_DISABLED_CONTROL_NORMAL (pad 0): the input even while disabled.
pub fn get_disabled_control_normal(action: i32) -> f32 {
    ret_f32(call(0x11E65974A982637C, args!(0, action)))
}


// ---- Drivability damage ----

/// GET_WORLD_POSITION_OF_ENTITY_BONE
pub fn get_world_position_of_entity_bone(entity: i32, bone: i32) -> Vec3 {
    unsafe { *(call(0x44A8FCB8ED227738, args!(entity, bone)) as *const Vec3) }
}

/// GET_OFFSET_FROM_ENTITY_GIVEN_WORLD_COORDS: a world point in the entity's
/// own space (x right, y forward, z up).
pub fn get_offset_from_entity_given_world_coords(entity: i32, p: Vec3) -> Vec3 {
    unsafe { *(call(0x2274BC1C4885E333, args!(entity, p.x, p.y, p.z)) as *const Vec3) }
}

/// SET_VEHICLE_TYRES_CAN_BURST (false: tyres can't be popped).
pub fn set_vehicle_tyres_can_burst(vehicle: i32, on: bool) {
    call(0xEB9DC3C7D8596C46, args!(vehicle, on));
}

/// SET_VEHICLE_DENSITY_MULTIPLIER_THIS_FRAME: amount of traffic (1 = normal).
pub fn set_vehicle_density_multiplier_this_frame(m: f32) {
    call(0x245A6883D966D537, args!(m));
}

pub fn set_random_vehicle_density_multiplier_this_frame(m: f32) {
    call(0xB3B3359379FE77D3, args!(m));
}

/// SET_VEHICLE_HANDBRAKE: holds the handbrake on, whatever the driver does.
pub fn set_vehicle_handbrake(vehicle: i32, on: bool) {
    call(0x684785568EF26A22, args!(vehicle, on));
}

/// GET_ENTITY_MATRIX: the entity's right, forward and up directions (world).
pub fn get_entity_matrix(entity: i32) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let (mut f, mut r, mut u, mut p) = (Vec3::default(), Vec3::default(), Vec3::default(), Vec3::default());
    call(0xECB2FC7235A7D137, args!(entity, &mut f as *mut Vec3, &mut r as *mut Vec3, &mut u as *mut Vec3, &mut p as *mut Vec3));
    ([r.x, r.y, r.z], [f.x, f.y, f.z], [u.x, u.y, u.z])
}

/// GET_ENTITY_HEIGHT_ABOVE_GROUND (m).
pub fn get_entity_height_above_ground(entity: i32) -> f32 {
    ret_f32(call(0x1DD55701034110E5, args!(entity)))
}

/// SET_ENTITY_NO_COLLISION_ENTITY: the two don't collide (this frame only,
/// or until turned back on).
pub fn set_entity_no_collision_entity(a: i32, b: i32, this_frame: bool) {
    call(0xA53ED5520C07654A, args!(a, b, this_frame));
}
