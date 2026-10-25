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
    draw_text_outlined(text, x, y, scale, rgba, true);
}

/// draw_text, with or without the dark outline (none for dark text on a
/// light background).
pub fn draw_text_outlined(text: &str, x: f32, y: f32, scale: f32, rgba: [i32; 4], outline: bool) {
    let entry = CString::new("STRING").unwrap();
    let body = CString::new(text.replace('\0', "")).unwrap();
    call(0x66E0276CC5F6B9DA, args!(4)); // SET_TEXT_FONT (4 = condensed)
    call(0x07C837F9A01C34C9, args!(scale, scale)); // SET_TEXT_SCALE
    call(0xBE6B23FFA53FB442, args!(rgba[0], rgba[1], rgba[2], rgba[3])); // SET_TEXT_COLOUR
    if outline {
        call(0x2513DFB0FB8400FE, &[]); // SET_TEXT_OUTLINE
    }
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

/// GET_NTH_CLOSEST_VEHICLE_NODE_WITH_HEADING (`nth` from 1 = the nearest).
pub fn nth_closest_vehicle_node(p: Vec3, nth: i32) -> Option<(Vec3, f32)> {
    let mut out = Vec3::default();
    let mut heading = 0f32;
    let mut lanes = 0i32;
    let ok = ret_bool(call(
        0x80CA6A8B6C094CC4,
        args!(p.x, p.y, p.z, nth, &mut out as *mut Vec3, &mut heading as *mut f32, &mut lanes as *mut i32, 1, 3.0f32, 0.0f32),
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
    /// Custom (junkyard respray) primary and secondary colours.
    custom: [Option<[i32; 3]>; 2],
    finish: i32,
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
    l.custom = [custom_colour(v, false), custom_colour(v, true)];
    l.finish = paint_finish(v);
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
    set_paint_finish(v, l.finish);
    for (secondary, c) in [(false, l.custom[0]), (true, l.custom[1])] {
        if c.is_some() {
            set_custom_colour(v, secondary, c);
        }
    }
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

/// SET_DRIVE_TASK_CRUISE_SPEED and SET_DRIVE_TASK_MAX_CRUISE_SPEED (m/s) for
/// the driver's current drive task.
pub fn set_drive_task_cruise_speed(driver: i32, speed: f32) {
    call(0x404A5AA9B9F0B746, args!(driver, speed, true));
    call(0x5C9B84BD7D31D908, args!(driver, speed));
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

/// UPDATE_WANTED_POSITION_THIS_FRAME: the police's idea of where the
/// player is becomes where they are now.
pub fn update_wanted_position_this_frame(player: i32) {
    call(0xBC9490CA15AEA8FB, args!(player));
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

pub fn is_ped_ragdoll(ped: i32) -> bool {
    ret_bool(call(0x47E4E977581C5B55, args!(ped)))
}

pub fn point_cam_at_coord(cam: i32, p: Vec3) {
    call(0xF75497BB865F0803, args!(cam, p.x, p.y, p.z));
}

/// IS_MODEL_IN_CDIMAGE: the game has this model.
pub fn is_model_in_cdimage(model: u32) -> bool {
    ret_bool(call(0x35B9E0803292B641, args!(model)))
}

pub fn is_model_a_vehicle(model: u32) -> bool {
    ret_bool(call(0x19AAC8F07BFEC53E, args!(model)))
}

pub fn is_waypoint_active() -> bool {
    ret_bool(call(0x1DD1F58F493F1DA5, &[]))
}

pub fn first_blip_info_id(sprite: i32) -> i32 {
    ret_i32(call(0x1BEDE233E6CD2A1F, args!(sprite)))
}

pub fn blip_info_coords(blip: i32) -> Vec3 {
    unsafe { *(call(0xFA7C7F0AADF25D09, args!(blip)) as *const Vec3) }
}

/// SET_VEHICLE_OUT_OF_CONTROL: its driver stops driving it (a takedown).
pub fn set_vehicle_out_of_control(vehicle: i32, kill_driver: bool, explode_on_impact: bool) {
    call(0xF19D095E42D430CC, args!(vehicle, kill_driver, explode_on_impact));
}

/// SET_ENTITY_PROOFS with only fire, explosion and collision set.
pub fn set_entity_proofs(entity: i32, fire: bool, explosion: bool, collision: bool) {
    call(0xFAEE099C6F890BB8, args!(entity, false, fire, explosion, collision, false, false, false, false));
}

/// Like draw_text, centred on `x`.
pub fn draw_text_centred(text: &str, x: f32, y: f32, scale: f32, rgba: [i32; 4]) {
    call(0xC02F4DBFB51D988B, args!(true)); // SET_TEXT_CENTRE
    draw_text(text, x, y, scale, rgba);
}

/// CLEAR_PLAYER_WANTED_LEVEL
pub fn clear_player_wanted_level(player: i32) {
    call(0xB302540597885499, args!(player));
}

pub fn get_max_wanted_level() -> i32 {
    ret_i32(call(0x462E0DB9B137DC5F, &[]))
}

/// SET_MAX_WANTED_LEVEL: 0 = the player can't be wanted at all.
pub fn set_max_wanted_level(level: i32) {
    call(0xAA5F02DB48D704B9, args!(level));
}

// ---- Junctions ----

/// BRING_VEHICLE_TO_HALT: the vehicle brakes to a stop within `distance` m
/// and stays stopped for `duration` (its driver's own driving pauses).
pub fn bring_vehicle_to_halt(vehicle: i32, distance: f32, duration: i32) {
    call(0x260BE8F09E326A20, args!(vehicle, distance, duration, false));
}

/// STOP_BRINGING_VEHICLE_TO_HALT
pub fn stop_bringing_vehicle_to_halt(vehicle: i32) {
    call(0x7C06330BFDDA182E, args!(vehicle));
}

/// IS_VEHICLE_BEING_BROUGHT_TO_HALT
pub fn is_vehicle_being_brought_to_halt(vehicle: i32) -> bool {
    ret_bool(call(0xC69BB1D832A710EF, args!(vehicle)))
}

/// SET_VEHICLE_MAX_SPEED (0 or less: no limit again).
pub fn set_vehicle_max_speed(vehicle: i32, speed: f32) {
    call(0xBAA045B4E42F3C06, args!(vehicle, speed));
}

/// SET_VEHICLE_BRAKE_LIGHTS (this frame).
pub fn set_vehicle_brake_lights(vehicle: i32, on: bool) {
    call(0x92B35082E0B42F66, args!(vehicle, on));
}

/// SET_ENTITY_TRAFFICLIGHT_OVERRIDE: 0 green, 1 red, 2 amber, 3 back to normal.
pub fn set_entity_trafficlight_override(entity: i32, state: i32) {
    call(0x57C5DB656185EAC4, args!(entity, state));
}

pub fn is_vehicle_siren_on(vehicle: i32) -> bool {
    ret_bool(call(0x4C9BF537BE2634B2, args!(vehicle)))
}

/// IS_ENTITY_ON_SCREEN (in view, even behind a wall).
pub fn is_entity_on_screen(entity: i32) -> bool {
    ret_bool(call(0xE659E47AF827484B, args!(entity)))
}

/// DRAW_MARKER: a marker of `kind` (28 = sphere) this frame.
pub fn draw_marker(kind: i32, p: Vec3, scale: f32, rgba: [i32; 4]) {
    let none = std::ptr::null::<std::ffi::c_char>();
    call(
        0x28477EC23D892089,
        args!(kind, p.x, p.y, p.z, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, scale, scale, scale, rgba[0], rgba[1], rgba[2], rgba[3], false, false, 2, false, none, none, false),
    );
}

/// DRAW_LIGHT_WITH_RANGE: a point light this frame.
pub fn draw_light_with_range(p: Vec3, rgb: [i32; 3], range: f32, intensity: f32) {
    call(0xF2A1B2771A01DBD4, args!(p.x, p.y, p.z, rgb[0], rgb[1], rgb[2], range, intensity));
}

// ---- GTA's missions and blips ----

pub fn script_thread_iterator_reset() {
    call(0xDADFADA5A20143A8, &[]);
}

/// SCRIPT_THREAD_ITERATOR_GET_NEXT_THREAD_ID: 0 when there are no more.
pub fn script_thread_next() -> i32 {
    ret_i32(call(0x30B4FA1C82DD4B9F, &[]))
}

/// GET_NAME_OF_SCRIPT_WITH_THIS_ID
pub fn script_name(thread: i32) -> Option<String> {
    c_str(call(0x05A42BA9FC8DA96B, args!(thread)))
}

pub fn terminate_thread(thread: i32) {
    call(0xC8B189ED9138BCD4, args!(thread));
}

pub fn next_blip_info_id(sprite: i32) -> i32 {
    ret_i32(call(0x14F96AA50D6FBEA7, args!(sprite)))
}

pub fn does_blip_exist(blip: i32) -> bool {
    ret_bool(call(0xA6DB27D19ECBB7DA, args!(blip)))
}

/// GET_BLIP_INFO_ID_TYPE: 1 vehicle, 2 ped, 3 object, 4 coord, 6 pickup, 7 radius.
pub fn blip_info_type(blip: i32) -> i32 {
    ret_i32(call(0xBE9B0959FFD0779B, args!(blip)))
}

/// GET_BLIP_INFO_ID_DISPLAY (0 hidden).
pub fn blip_display(blip: i32) -> i32 {
    ret_i32(call(0x1E314167F701DC3B, args!(blip)))
}

/// SET_BLIP_DISPLAY (0 hidden, 2 map and radar, ...).
pub fn set_blip_display(blip: i32, display: i32) {
    call(0x9029B2F3DA924928, args!(blip, display));
}

pub fn add_blip_for_coord(p: Vec3) -> i32 {
    ret_i32(call(0x5A039BB0BCA604B6, args!(p.x, p.y, p.z)))
}

pub fn set_blip_sprite(blip: i32, sprite: i32) {
    call(0xDF735600A4696DAF, args!(blip, sprite));
}

/// On the radar only when near (always on the pause map).
pub fn set_blip_as_short_range(blip: i32, on: bool) {
    call(0xBE8BE4FE60E27B72, args!(blip, on));
}

/// The blip's name in the pause map legend.
pub fn set_blip_name(blip: i32, name: &str) {
    let (entry, body) = (CString::new("STRING").unwrap(), CString::new(name).unwrap());
    call(0xF9113A30DE5C6670, args!(entry.as_ptr()));
    call(0x6C188BE134E074AA, args!(body.as_ptr()));
    call(0xBC38B49BCB83BC9B, args!(blip));
}

pub fn remove_blip(blip: i32) {
    let mut b = blip;
    call(0x86A652570E5F25DD, args!(&mut b as *mut i32));
}

// ---- Shops ----

/// SET_VEHICLE_FIXED, SET_VEHICLE_DEFORMATION_FIXED and a wash: as new.
pub fn repair_vehicle(vehicle: i32) {
    call(0x115722B1B9C14C1C, args!(vehicle));
    call(0x953DA1E1B12C0491, args!(vehicle));
    call(0x79D3B596FE44EE8B, args!(vehicle, 0.0f32));
}

pub fn set_vehicle_mod_kit(vehicle: i32) {
    call(0x1F2AA07F00B3217A, args!(vehicle, 0));
}

pub fn num_vehicle_mods(vehicle: i32, kind: i32) -> i32 {
    ret_i32(call(0xE38E9162A2500646, args!(vehicle, kind)))
}

/// GET_VEHICLE_MOD: -1 for stock.
pub fn vehicle_mod(vehicle: i32, kind: i32) -> i32 {
    ret_i32(call(0x772960298DA26FDB, args!(vehicle, kind)))
}

/// SET_VEHICLE_MOD; -1 is back to stock (REMOVE_VEHICLE_MOD).
pub fn set_vehicle_mod(vehicle: i32, kind: i32, index: i32) {
    if index < 0 {
        call(0x92D619E420858204, args!(vehicle, kind));
    } else {
        call(0x6AF0636DDEDCB6DD, args!(vehicle, kind, index, false));
    }
}

/// A returned C string, if not null or empty.
fn c_str(p: *mut u64) -> Option<String> {
    let p = unsafe { *(p as *const *const std::ffi::c_char) };
    if p.is_null() {
        return None;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned();
    (!s.is_empty()).then_some(s)
}

/// The game's text for a text label (GET_FILENAME_FOR_AUDIO_CONVERSATION,
/// the old _GET_LABEL_TEXT), if it has one.
pub fn label_text(label: &str) -> Option<String> {
    let c = CString::new(label).ok()?;
    if !ret_bool(call(0xAC09CA973C564252, args!(c.as_ptr()))) {
        return None; // DOES_TEXT_LABEL_EXIST
    }
    c_str(call(0x7B5280EBA9840C72, args!(c.as_ptr())))
}

/// A mod option's name (GET_MOD_TEXT_LABEL, then its text).
pub fn mod_name(vehicle: i32, kind: i32, index: i32) -> Option<String> {
    label_text(&c_str(call(0x8935624F8C5592CC, args!(vehicle, kind, index)))?)
}

/// The vehicle's own name for a mod slot (GET_MOD_SLOT_NAME), if it has one.
pub fn mod_slot_name(vehicle: i32, kind: i32) -> Option<String> {
    let label = c_str(call(0x51F0FEB9F6AE98C0, args!(vehicle, kind)))?;
    label_text(&label).or(Some(label))
}

pub fn toggle_vehicle_mod(vehicle: i32, kind: i32, on: bool) {
    call(0x2A1F4F37F95BAD08, args!(vehicle, kind, on));
}

pub fn is_toggle_mod_on(vehicle: i32, kind: i32) -> bool {
    ret_bool(call(0x84B233A8C8FC8AE7, args!(vehicle, kind)))
}

pub fn vehicle_wheel_type(vehicle: i32) -> i32 {
    ret_i32(call(0xB3ED1BFB4BE636DC, args!(vehicle)))
}

pub fn set_vehicle_wheel_type(vehicle: i32, kind: i32) {
    call(0x487EB21CC7295BA1, args!(vehicle, kind));
}

pub fn vehicle_window_tint(vehicle: i32) -> i32 {
    ret_i32(call(0x0EE21293DAD47C95, args!(vehicle)))
}

pub fn set_vehicle_window_tint(vehicle: i32, tint: i32) {
    call(0x57C51E6BAD752696, args!(vehicle, tint));
}

pub fn plate_style(vehicle: i32) -> i32 {
    ret_i32(call(0xF11BC2DD9A3E7195, args!(vehicle)))
}

pub fn set_plate_style(vehicle: i32, style: i32) {
    call(0x9088EB5A43FFB0A1, args!(vehicle, style));
}

pub fn livery_count(vehicle: i32) -> i32 {
    ret_i32(call(0x87B63E25A529D526, args!(vehicle)))
}

pub fn livery(vehicle: i32) -> i32 {
    ret_i32(call(0x2BB9230590DA5E8A, args!(vehicle)))
}

pub fn set_livery(vehicle: i32, livery: i32) {
    call(0x60BF608F1B8CD1B6, args!(vehicle, livery));
}

/// SET_VEHICLE_CUSTOM_PRIMARY_COLOUR / _SECONDARY_; None goes back to the
/// palette colour (CLEAR_VEHICLE_CUSTOM_..._COLOUR).
pub fn set_custom_colour(vehicle: i32, secondary: bool, rgb: Option<[i32; 3]>) {
    match (secondary, rgb) {
        (false, Some(c)) => call(0x7141766F91D15BEA, args!(vehicle, c[0], c[1], c[2])),
        (true, Some(c)) => call(0x36CED73BFED89754, args!(vehicle, c[0], c[1], c[2])),
        (false, None) => call(0x55E1D2758F34E437, args!(vehicle)),
        (true, None) => call(0x5FFBDEEC3E8E2009, args!(vehicle)),
    };
}

pub fn custom_colour(vehicle: i32, secondary: bool) -> Option<[i32; 3]> {
    let (is, get) = if secondary { (0x910A32E7AAD2656C, 0x8389CD56CA8072DC) } else { (0xF095C0405307B21B, 0xB64CF2CCA9D95F52) };
    if !ret_bool(call(is, args!(vehicle))) {
        return None;
    }
    let mut c = [0i32; 3];
    let [r, g, b] = &mut c;
    call(get, args!(vehicle, r as *mut i32, g as *mut i32, b as *mut i32));
    Some(c)
}

/// The primary paint's finish (GET_VEHICLE_MOD_COLOR_1's paint type: 0
/// normal, 1 metallic, 2 pearlescent, 3 matte, 4 metal, 5 chrome).
pub fn paint_finish(vehicle: i32) -> i32 {
    let (mut kind, mut colour, mut pearl) = (0i32, 0i32, 0i32);
    call(0xE8D65CA700C9A693, args!(vehicle, &mut kind as *mut i32, &mut colour as *mut i32, &mut pearl as *mut i32));
    kind
}

/// SET_VEHICLE_MOD_COLOR_1, keeping the colour it has.
pub fn set_paint_finish(vehicle: i32, finish: i32) {
    let (mut kind, mut colour, mut pearl) = (0i32, 0i32, 0i32);
    call(0xE8D65CA700C9A693, args!(vehicle, &mut kind as *mut i32, &mut colour as *mut i32, &mut pearl as *mut i32));
    call(0x43FEB945EE7F85B8, args!(vehicle, finish, colour, pearl));
}

/// DISABLE_ALL_CONTROL_ACTIONS (pad 0) for this frame.
pub fn disable_all_controls() {
    call(0x5F4B6931816E599B, args!(0));
}

/// ENABLE_CONTROL_ACTION (pad 0) for this frame.
pub fn enable_control(action: i32) {
    call(0x351220255D64C155, args!(0, action, true));
}

/// IS_DISABLED_CONTROL_JUST_PRESSED (pad 0)
pub fn is_disabled_control_just_pressed(action: i32) -> bool {
    ret_bool(call(0x91AEF906BCA88877, args!(0, action)))
}

/// GET_VEHICLE_CLASS_FROM_NAME (as GET_VEHICLE_CLASS, from the model).
pub fn vehicle_class_from_name(model: u32) -> i32 {
    ret_i32(call(0xDEDF1C8BD47C2200, args!(model)))
}

/// A vehicle model's name in the game ("Adder"), from its display name label.
pub fn display_name(model: u32) -> Option<String> {
    label_text(&c_str(call(0xB215AAC32D25D019, args!(model)))?)
}

/// A vehicle model's maker ("Truffade"), from its make name label.
pub fn make_name(model: u32) -> Option<String> {
    label_text(&c_str(call(0xF7AF4F159FF99F97, args!(model)))?)
}

/// A vehicle model's stock performance, as GTA's own stat bars use it:
/// GET_VEHICLE_MODEL_ESTIMATED_MAX_SPEED (m/s), _ACCELERATION,
/// _MAX_BRAKING and _MAX_TRACTION.
pub fn model_performance(model: u32) -> [f32; 4] {
    [
        ret_f32(call(0xF417C2502FFFED43, args!(model))),
        ret_f32(call(0x8C044C5C84505B6A, args!(model))),
        ret_f32(call(0xDC53FD41B4ED944C, args!(model))),
        ret_f32(call(0x539DE94D44FDFD0D, args!(model))),
    ]
}

/// SET_ENTITY_VISIBLE
pub fn set_entity_visible(entity: i32, on: bool) {
    call(0xEA1C610A04DB6BBB, args!(entity, on, false));
}

/// SET_ENTITY_COLLISION (keeping physics).
pub fn set_entity_collision(entity: i32, on: bool) {
    call(0x1A9205C1B9EE827F, args!(entity, on, true));
}

/// GET_MODEL_DIMENSIONS: the model's bounding box (min, max).
pub fn model_dimensions(model: u32) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::default(), Vec3::default());
    call(0x03E8D3D5F549087A, args!(model, &mut lo as *mut Vec3, &mut hi as *mut Vec3));
    (lo, hi)
}

/// GET_VEHICLE_COLOURS: (primary, secondary) palette indices.
pub fn vehicle_colours(vehicle: i32) -> (i32, i32) {
    let (mut a, mut b) = (0i32, 0i32);
    call(0xA19435F193E081AC, args!(vehicle, &mut a as *mut i32, &mut b as *mut i32));
    (a, b)
}

pub fn set_vehicle_colours(vehicle: i32, primary: i32, secondary: i32) {
    call(0x4F1D4BE3A7F24601, args!(vehicle, primary, secondary));
}

/// GET_VEHICLE_EXTRA_COLOURS: (pearlescent, wheel) palette indices.
pub fn vehicle_extra_colours(vehicle: i32) -> (i32, i32) {
    let (mut a, mut b) = (0i32, 0i32);
    call(0x3BC4245933A166F7, args!(vehicle, &mut a as *mut i32, &mut b as *mut i32));
    (a, b)
}

pub fn set_vehicle_extra_colours(vehicle: i32, pearlescent: i32, wheel: i32) {
    call(0x2036F561ADD12E33, args!(vehicle, pearlescent, wheel));
}

/// One-shot particle effect at a point (from `asset`), turned `heading`
/// degrees about the up axis.
pub fn ptfx_at(asset: &str, effect: &str, p: Vec3, heading: f32, scale: f32) {
    let a = CString::new(asset).unwrap();
    let e = CString::new(effect).unwrap();
    call(0x6C38AF3693A69A91, args!(a.as_ptr())); // USE_PARTICLE_FX_ASSET
    call(0x25129531F77B9ED3, args!(e.as_ptr(), p.x, p.y, p.z, 0.0f32, 0.0f32, heading, scale, false, false, false));
}

/// START_PARTICLE_FX_LOOPED_ON_ENTITY (from `asset`), at an offset in the
/// entity's own frame; the effect's handle (0: none).
pub fn ptfx_looped_on_entity(asset: &str, effect: &str, entity: i32, offset: Vec3, scale: f32) -> i32 {
    let a = CString::new(asset).unwrap();
    let e = CString::new(effect).unwrap();
    call(0x6C38AF3693A69A91, args!(a.as_ptr())); // USE_PARTICLE_FX_ASSET
    ret_i32(call(0x1AE42C1660FD6517, args!(e.as_ptr(), entity, offset.x, offset.y, offset.z, 0.0f32, 0.0f32, 0.0f32, scale, false, false, false)))
}

/// SET_PARTICLE_FX_LOOPED_EVOLUTION
pub fn set_ptfx_evolution(handle: i32, name: &str, amount: f32) {
    let c = CString::new(name).unwrap();
    call(0x5F0C4B5B1C393BE2, args!(handle, c.as_ptr(), amount, false));
}

/// SET_PARTICLE_FX_LOOPED_OFFSETS (no rotation).
pub fn set_ptfx_offset(handle: i32, offset: Vec3) {
    call(0xF7DDEBEC43483C43, args!(handle, offset.x, offset.y, offset.z, 0.0f32, 0.0f32, 0.0f32));
}

pub fn does_ptfx_exist(handle: i32) -> bool {
    ret_bool(call(0x74AFEF0D2E1E409B, args!(handle)))
}

pub fn stop_ptfx(handle: i32) {
    call(0x8F75998877616996, args!(handle, false));
}

/// The gameplay camera doesn't collide with `entity` this frame
/// (DISABLE_CAM_COLLISION_FOR_OBJECT and
/// SET_GAMEPLAY_CAM_IGNORE_ENTITY_COLLISION_THIS_UPDATE).
pub fn camera_ignore(entity: i32) {
    call(0x49482F9FCD825AAA, args!(entity));
    call(0x2AED6301F67007D5, args!(entity));
}

// ---- GPS (our own route, drawn as a custom GPS route) ----

/// START_GPS_CUSTOM_ROUTE, the points (straight lines between them), then
/// SET_GPS_CUSTOM_ROUTE_RENDER; drawn on the radar and the pause map.
pub fn gps_custom_route(hud_colour: i32, points: &[Vec3], radar_width: i32, map_width: i32) {
    call(0xE6DE0561D9232A64, &[]); // CLEAR_GPS_CUSTOM_ROUTE
    call(0xDB34E8D56FC13B08, args!(hud_colour, true, true));
    for p in points {
        call(0x311438A071DD9B1A, args!(p.x, p.y, p.z));
    }
    call(0x900086F371220B6F, args!(true, radar_width, map_width));
}

pub fn clear_gps_custom_route() {
    call(0x900086F371220B6F, args!(false, 0, 0)); // SET_GPS_CUSTOM_ROUTE_RENDER
    call(0xE6DE0561D9232A64, &[]);
}

// ---- Character switching ----

pub fn get_player_switch_state() -> i32 {
    ret_i32(call(0x470555300D10B2A5, &[]))
}

pub fn get_player_switch_type() -> i32 {
    ret_i32(call(0xB3C94A90D9FC9E62, &[]))
}

pub fn is_switch_ready_for_descent() -> bool {
    ret_bool(call(0xDFA80CB25D0A19B3, &[]))
}

pub fn allow_player_switch_descent() {
    call(0xAD5FDF34B81BFE79, &[]);
}

/// Lets the switch's final swoop onto the new character start (it holds in
/// state 8 until a script allows it).
pub fn allow_player_switch_outro() {
    call(0x74DE2E8739086740, &[]);
}

/// CHANGE_PLAYER_PED: the player takes over another ped.
pub fn change_player_ped(ped: i32) {
    call(0x048189FAC643DEEE, args!(player_id(), ped, false, false));
}

/// CLEAR_PED_TASKS (not _IMMEDIATELY, which would pull them out of the car).
pub fn clear_ped_tasks(ped: i32) {
    call(0xE1EF3C1216AFF2CD, args!(ped));
}

/// SET_ENTITY_LOAD_COLLISION_FLAG: collision streams in around the entity
/// wherever the player is.
pub fn set_entity_load_collision_flag(entity: i32, on: bool) {
    call(0x0DC7CABAB1E9B67E, args!(entity, on, 1));
}

/// GET_FINAL_RENDERED_CAM_COORD: where the camera drawn this frame is.
pub fn get_final_rendered_cam_coord() -> Vec3 {
    unsafe { *(call(0xA200EB1EE790F448, &[]) as *const Vec3) }
}

/// SET_ENTITY_AS_NO_LONGER_NEEDED: the game may clean the entity up once
/// it's out of sight.
pub fn set_entity_as_no_longer_needed(entity: i32) {
    let mut e = entity;
    call(0xB736A491E64A32CF, args!(&mut e as *mut i32));
}

/// STOP_SCRIPTED_CONVERSATION: the dialogue a script started stops now.
pub fn stop_scripted_conversation() {
    call(0xD79DEEFB53455EBA, args!(false));
}

/// After a drunk scene: STOP_GAMEPLAY_CAM_SHAKING, SET_PED_IS_DRUNK off and
/// RESET_PED_MOVEMENT_CLIPSET.
pub fn sober_up(ped: i32) {
    call(0x0EF93E9F3D08C178, args!(true));
    call(0x95D2D383D5396B8A, args!(ped, false));
    call(0xAA74EC0CB0AAEA2C, args!(ped, 0.0f32));
}

pub fn task_vehicle_drive_wander(ped: i32, vehicle: i32, speed: f32, style: i32) {
    call(0x480142959D337D00, args!(ped, vehicle, speed, style));
}

pub fn set_entity_as_mission_entity(entity: i32) {
    call(0xAD738C3085FE7E11, args!(entity, true, true));
}

pub fn set_player_control(on: bool) {
    call(0x8D32347D6D4C40A2, args!(player_id(), on, 0));
}

pub fn display_radar(on: bool) {
    call(0xA0EBB943C300E693, args!(on));
}

pub fn destroy_all_cams() {
    call(0x8E5FB15663F79120, args!(false));
}

// ---- Races ----

/// CREATE_PED_INSIDE_VEHICLE (seat -1: the driver).
pub fn create_ped_inside_vehicle(vehicle: i32, ped_type: i32, model: u32, seat: i32) -> i32 {
    ret_i32(call(0x7DD959874C1FD534, args!(vehicle, ped_type, model, seat, false, false)))
}

pub fn set_blocking_of_non_temporary_events(ped: i32, on: bool) {
    call(0x9F8AA94D6D97DBF4, args!(ped, on));
}

/// SET_PED_COMBAT_ATTRIBUTES (3 = can leave its vehicle).
pub fn set_ped_combat_attributes(ped: i32, attribute: i32, on: bool) {
    call(0x9F7794730795E019, args!(ped, attribute, on));
}

pub fn set_ped_can_be_dragged_out(ped: i32, on: bool) {
    call(0xC1670E958EEE24E5, args!(ped, on));
}

pub fn set_ped_stay_in_vehicle_when_jacked(ped: i32, on: bool) {
    call(0xEDF4079F9D54C9A1, args!(ped, on));
}

pub fn set_ped_get_out_upside_down_vehicle(ped: i32, on: bool) {
    call(0xBC0ED94165A48BC2, args!(ped, on));
}

/// GET_IS_TASK_ACTIVE (2 = TASK_EXIT_VEHICLE).
pub fn get_is_task_active(ped: i32, task: i32) -> bool {
    ret_bool(call(0xB0760331C7AA4155, args!(ped, task)))
}

pub fn set_ped_keep_task(ped: i32, on: bool) {
    call(0x971D38760FBC02EF, args!(ped, on));
}

/// CREATE_CHECKPOINT: `kind` as in the native's list (0: cylinder with an
/// arrow pointing at `next`, 4: chequered flag); the game draws it until
/// it's deleted.
pub fn create_checkpoint(kind: i32, p: Vec3, next: Vec3, diameter: f32, rgba: [i32; 4]) -> i32 {
    ret_i32(call(0x0134F0835AB6BFCB, args!(kind, p.x, p.y, p.z, next.x, next.y, next.z, diameter, rgba[0], rgba[1], rgba[2], rgba[3], 0)))
}

pub fn delete_checkpoint(checkpoint: i32) {
    call(0xF5ED37F54CD4D52E, args!(checkpoint));
}

pub fn set_blip_colour(blip: i32, colour: i32) {
    call(0x03D7FB09E75D6B7E, args!(blip, colour));
}

/// A blip that follows an entity around the map.
pub fn add_blip_for_entity(entity: i32) -> i32 {
    ret_i32(call(0x5CDE92C702A8FCE7, args!(entity)))
}

/// The cone showing which way a blip's entity faces (GTA Online's players).
pub fn show_heading_indicator_on_blip(blip: i32, on: bool) {
    call(0x5FBCA48327B914DF, args!(blip, on));
}

pub fn show_number_on_blip(blip: i32, number: i32) {
    call(0xA3C0B359DCB848B6, args!(blip, number));
}

pub fn set_blip_scale(blip: i32, scale: f32) {
    call(0xD38744167B2FA257, args!(blip, scale));
}

/// GET_RANDOM_INT_IN_RANGE: lo..hi (hi not included).
pub fn random_int(lo: i32, hi: i32) -> i32 {
    ret_i32(call(0xD53343AA4FB7DD28, args!(lo, hi)))
}

pub fn get_hud_colour(index: i32) -> [i32; 4] {
    let (mut r, mut g, mut b, mut a) = (0i32, 0i32, 0i32, 0i32);
    call(0x7C9C91AB74A0360F, args!(index, &mut r as *mut i32, &mut g as *mut i32, &mut b as *mut i32, &mut a as *mut i32));
    [r, g, b, a]
}

pub fn replace_hud_colour(index: i32, rgba: [i32; 4]) {
    call(0xF314CF4F0211894E, args!(index, rgba[0], rgba[1], rgba[2], rgba[3]));
}

pub fn is_pause_menu_active() -> bool {
    ret_bool(call(0xB0034A223497FFCB, &[]))
}
