//! paradise.asi: in-game companion for the gta-paradise mod.
//!
//! Current role: a collision/streaming diagnostic.
//! - F6 cycles through teleport destinations listed in `paradise.ini` (game
//!   folder; re-read on every press), holds the player in place while
//!   requesting collision, then probes for ground below.
//! - F5 releases a hold (if no ground was found).
//! - F7 toggles the on-screen overlay.
//! - Story mode does not apply DLC map change sets by default, so once the
//!   game has loaded this calls ON_ENTER_SP, which applies GROUP_MAP_SP where
//!   the Paradise map lives (and, when built that way, removes Los Santos).
//!   The player is moved to `@start` first so nothing is standing in Los
//!   Santos while it is swapped out. `@mode off` in paradise.ini skips all of
//!   this, leaving normal GTA. F10 calls ON_ENTER_SP again. (ON_ENTER_MP is deliberately
//!   not offered: loading Online map content crashed this install.)
//! - F4 toggles the collision overlay (nearby Paradise collision drawn as
//!   translucent coloured surfaces, from `paradise_col.bin`); off at start.
//! - Respawn: "wasted" and "busted" play out as in GTA (penalties too), but
//!   the screen is held black over GTA's respawn at a hospital / police
//!   station (in removed Los Santos) and the player comes back on foot near
//!   where it happened. 1.5 s with no ground below (not flying, not in water) puts the
//!   player back on `@start`.
//! - Burnout-style crashes (`wreck.rs`): hard impacts dent and smash the
//!   player's car; a wreck destroys it and respawns the player on the nearest
//!   road in a fresh copy. Settings: `@wreck`, `@wreck_speed`,
//!   `@impact_speed`, `@wreck_explode`, `@respawn_speed`, `@crash_time`,
//!   `@slowmo`, `@crash_cam`.
//! - Burnout-style boost (`boost.rs`): A / X while driving (GTA's duck)
//!   burns a gauge for speed and exhaust flames; driving fast, near misses,
//!   drifts and jumps refill it. Settings: `@boost...`.
//! - Spawn guard (`spawnguard.rs`): Los Santos leftovers (NPCs and vehicles
//!   appearing in mid-air, or inside no-spawn zones placed in the viewer)
//!   are removed and logged to `paradise_spawns.csv`.
//! - Police chase recorder (`chase.rs`): pursuing police while wanted, once
//!   a second, to `paradise_chase.csv`.
//! - Heavier jumps and downforce (`physics.rs`): airborne cars near the
//!   player fall `@air_gravity` times faster than GTA's gravity; fast cars
//!   on the ground are pressed down by `@downforce`.
//! - F9 marks the current spot (F8 is Menyoo's menu key): appends position, heading and whether in a
//!   vehicle to `paradise_marks.csv`, so a place can be pointed out exactly.
//! - While in a vehicle, motion is sampled at ~20 Hz into
//!   `paradise_trace.csv` (position, velocity, airborne) so bumps can be
//!   matched to the collision geometry offline.
//! - Everything else is logged to `paradise.log` in the game folder.

mod boost;
mod perf;
mod chase;
mod natives;
mod overlay;
mod physics;
mod propshield;
mod spawnguard;
mod shv;
mod wreck;

use natives as n;
use shv::Vec3;
use std::ffi::c_void;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::atomic::{AtomicPtr, Ordering};

static MODULE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(vkey: i32) -> i16;
}

const VK_F4: i32 = 0x73;
const VK_F5: i32 = 0x74;
const VK_F6: i32 = 0x75;
const VK_F7: i32 = 0x76;
const VK_F9: i32 = 0x78;
const VK_F10: i32 = 0x79;

#[no_mangle]
pub extern "system" fn DllMain(module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        1 => {
            MODULE.store(module, Ordering::SeqCst);
            unsafe { shv::script_register(module, script_main) };
        }
        0 => unsafe { shv::script_unregister(MODULE.load(Ordering::SeqCst)) },
        _ => {}
    }
    1
}

pub(crate) fn log(line: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("paradise.log") {
        let t = n::get_game_timer();
        let _ = writeln!(f, "[{:>9.3}s] {line}", t as f32 / 1000.0);
    }
}

struct Destination {
    name: String,
    pos: Vec3,
}

const DEFAULT_INI: &str = "\
# gta-paradise teleport destinations: name x y z (GTA world coordinates)
# F6 cycles through them; this file is re-read on every press.
paradise_road 994.9 862.7 1015.3
test_platform_low -150 6450 132
test_platform_high -150 6450 1017
";

fn load_destinations() -> Vec<Destination> {
    let text = std::fs::read_to_string("paradise.ini").unwrap_or_else(|_| {
        let _ = std::fs::write("paradise.ini", DEFAULT_INI);
        DEFAULT_INI.to_string()
    });
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim_start().starts_with('@'))
        .filter_map(|l| {
            let p: Vec<&str> = l.split_whitespace().collect();
            let f = |i: usize| p.get(i)?.parse::<f32>().ok();
            Some(Destination { name: p.first()?.to_string(), pos: Vec3::new(f(1)?, f(2)?, f(3)?) })
        })
        .collect()
}

/// `@key value...` settings from paradise.ini.
struct Settings {
    paradise: bool,
    start: Option<Vec3>,
    wreck: wreck::WreckSettings,
    boost: boost::BoostSettings,
    /// Gravity on airborne vehicles, times normal.
    air_gravity: f32,
    /// Extra downforce (g) on fast cars at ~180 km/h.
    downforce: f32,
    police: chase::PoliceSettings,
    /// Write the position probe and driving trace (debugging).
    debug_log: bool,
    /// Write the plugin's frame cost to paradise.log every 15 s.
    perf: bool,
}

fn load_settings() -> Settings {
    let text = std::fs::read_to_string("paradise.ini").unwrap_or_default();
    let mut s = Settings { paradise: true, start: None, wreck: Default::default(), boost: Default::default(), air_gravity: 2.0, downforce: 1.0, police: Default::default(), debug_log: false, perf: true };
    for line in text.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        let on = |v: &str| !(v.eq_ignore_ascii_case("off") || v == "0" || v.eq_ignore_ascii_case("false"));
        let num = |v: &str, d: f32| v.parse::<f32>().unwrap_or(d);
        let w = &mut s.wreck;
        let b = &mut s.boost;
        match p.as_slice() {
            ["@boost", v, ..] => b.enabled = on(v),
            ["@boost_accel", v, ..] => b.accel = num(v, b.accel),
            ["@boost_top", v, ..] => b.top_mult = num(v, b.top_mult),
            ["@boost_min_top", v, ..] => b.min_top = num(v, b.min_top),
            ["@boost_max_top", v, ..] => b.max_top = num(v, b.max_top),
            ["@boost_seconds", v, ..] => b.seconds = num(v, b.seconds),
            ["@boost_fill", v, ..] => b.fill = num(v, b.fill),
            ["@wreck", v, ..] => w.enabled = on(v),
            ["@wreck_speed", v, ..] => w.wreck_dv = num(v, w.wreck_dv),
            ["@impact_speed", v, ..] => w.impact_dv = num(v, w.impact_dv),
            ["@wreck_explode", v, ..] => w.explode = on(v),
            ["@respawn_speed", v, ..] => w.respawn_speed = num(v, w.respawn_speed),
            ["@crash_time", v, ..] => w.crash_time = num(v, w.crash_time),
            ["@slowmo", v, ..] => w.slowmo = num(v, w.slowmo).clamp(0.05, 1.0),
            ["@crash_cam", v, ..] => w.crash_cam = on(v),
            ["@precrash", v, ..] => w.precrash = on(v),
            ["@precrash_lead", v, ..] => w.precrash_lead = num(v, w.precrash_lead).clamp(0.05, 1.0),
            ["@wreck_damage", v, ..] => w.damage_wreck = on(v),
            ["@wreck_stuck", v, ..] => w.stuck_time = num(v, w.stuck_time),
            ["@prop_keep", v, ..] => w.prop_keep = num(v, w.prop_keep),
            ["@wreck_clear", v, ..] => w.clear_radius = num(v, w.clear_radius).max(0.0),
            ["@wreck_crush", v, ..] => w.crush = num(v, w.crush),
            ["@wreck_crush_rounds", v, ..] => w.crush_rounds = num(v, w.crush_rounds as f32) as i32,
            ["@air_gravity", v, ..] => s.air_gravity = num(v, s.air_gravity),
            ["@debug_log", v, ..] => s.debug_log = on(v),
            ["@perf", v, ..] => s.perf = on(v),
            ["@downforce", v, ..] => s.downforce = num(v, s.downforce).max(0.0),
            ["@police_power", v, ..] => s.police.power = num(v, s.police.power),
            ["@police_top", v, ..] => s.police.top = num(v, s.police.top),
            ["@mode", m, ..] => s.paradise = !m.eq_ignore_ascii_case("off"),
            ["@start", x, y, z, ..] => {
                if let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse()) {
                    s.start = Some(Vec3::new(x, y, z));
                }
            }
            _ => {}
        }
    }
    s
}

/// Is the base map still there? Probes straight down from high above `at`
/// (normally the Paradise start point): any ground well above Paradise's own
/// surface there can only be Los Santos.
fn base_map_check(at: Vec3) {
    let ped = n::player_ped_id();
    for _ in 0..60 {
        n::request_collision_at_coord(Vec3::new(at.x, at.y, 200.0));
        shv::wait(0);
    }
    match n::raycast(Vec3::new(at.x, at.y, 1200.0), Vec3::new(at.x, at.y, at.z - 50.0), 1, ped) {
        Some(h) if h.pos.z > at.z + 20.0 => log(&format!("base map check: LOS SANTOS STILL PRESENT (ground at z={:.1} above Paradise z={:.1})", h.pos.z, at.z)),
        Some(h) => log(&format!("base map check: only Paradise below (top ground z={:.1}) -> Los Santos removed", h.pos.z)),
        None => log("base map check: no ground at all below start"),
    }
}

/// Puts the player back somewhere sensible when the game leaves them
/// somewhere fatal: GTA still spawns, restores saves and respawns at
/// hospitals / police stations in removed Los Santos, over empty sea.
#[derive(Default)]
struct Rescue {
    was_dead: bool,
    /// Where the player died or was arrested.
    died_at: Option<Vec3>,
    /// The screen is being held black over GTA's respawn.
    holding: bool,
    /// Game time the player was first seen with no ground below.
    void_since: Option<i32>,
    last_probe: i32,
}

impl Rescue {
    /// How long the player may have nothing below before being rescued.
    const VOID_MS: i32 = 1500;

    fn check(&mut self, ped: i32, ent: i32, pos: Vec3, now: i32) -> Option<RescueKind> {
        if n::is_entity_dead(ped) || n::is_player_being_arrested(n::player_id()) {
            if !self.was_dead {
                self.died_at = Some(pos);
            }
            // GTA's "wasted" / "busted" plays out in full; once it has faded
            // to black, keep it black through the respawn in Los Santos.
            if n::is_screen_faded_out() {
                self.holding = true;
            }
            if self.holding {
                n::do_screen_fade_out(0);
            }
            self.was_dead = true;
            self.void_since = None;
            return None;
        }
        if self.was_dead {
            // Respawned (hospital / police station): those are in Los Santos.
            self.was_dead = false;
            return Some(RescueKind::Respawned(self.died_at.take(), std::mem::take(&mut self.holding)));
        }
        // Swimming or boating on the open sea is fine.
        if n::is_ped_in_flying_vehicle(ped) || n::is_entity_in_water(ent) || pos.z < 5.0 {
            self.void_since = None;
            return None;
        }
        if now - self.last_probe < 250 {
            return None;
        }
        self.last_probe = now;
        let ground = n::raycast(Vec3::new(pos.x, pos.y, pos.z + 2.0), Vec3::new(pos.x, pos.y, pos.z - 1000.0), 1, ent);
        match (ground, self.void_since) {
            (Some(_), _) => self.void_since = None,
            (None, None) => self.void_since = Some(now),
            (None, Some(t)) if now - t >= Self::VOID_MS => return Some(RescueKind::NoGround),
            _ => {}
        }
        None
    }
}

enum RescueKind {
    /// Where it happened, and whether the screen is being held black.
    Respawned(Option<Vec3>, bool),
    NoGround,
}

/// Ends the arrival scene the game plays on load (a player switch, or the
/// character's activity such as pull-ups) so the player can be moved.
fn end_arrival_scene() {
    if n::is_player_switch_in_progress() {
        n::stop_player_switch();
    }
    n::clear_ped_tasks_immediately(n::player_ped_id());
    n::render_script_cams(false, 0);
}

/// A spot to respawn on foot near `p`: the pedestrian navmesh, else the
/// nearest road.
fn respawn_near(p: Vec3) -> Option<Destination> {
    for _ in 0..30 {
        n::request_collision_at_coord(p);
        shv::wait(0);
    }
    let spot = n::safe_coord_for_ped(p).or_else(|| n::closest_vehicle_node(p).map(|(q, _)| q))?;
    let d = ((spot.x - p.x).powi(2) + (spot.y - p.y).powi(2)).sqrt();
    (d < 300.0).then(|| Destination { name: "near death".into(), pos: Vec3::new(spot.x, spot.y, spot.z + 1.0) })
}

/// Appends a marked spot to paradise_marks.csv; returns its number.
fn mark_spot(pos: Vec3, heading: f32, in_vehicle: bool) -> usize {
    const FILE: &str = "paradise_marks.csv";
    let existing = std::fs::read_to_string(FILE).unwrap_or_default();
    let n_mark = existing.lines().filter(|l| !l.starts_with("mark")).count() + 1;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(FILE) {
        if existing.is_empty() {
            let _ = writeln!(f, "mark,x,y,z,heading,in_vehicle");
        }
        let _ = writeln!(f, "{n_mark},{:.2},{:.2},{:.2},{:.1},{}", pos.x, pos.y, pos.z, heading, in_vehicle as u8);
    }
    n_mark
}

/// Edge-triggered key press.
struct Keys([bool; 256]);
impl Keys {
    fn pressed(&mut self, vk: i32) -> bool {
        let down = unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
        let was = std::mem::replace(&mut self.0[vk as usize], down);
        down && !was
    }
}

fn controlled_entity(ped: i32) -> i32 {
    if n::is_ped_in_any_vehicle(ped) {
        n::get_vehicle_ped_is_in(ped)
    } else {
        ped
    }
}

fn describe_probe(at: Vec3, ignore: i32) -> String {
    let from = Vec3::new(at.x, at.y, at.z + 50.0);
    let to = Vec3::new(at.x, at.y, at.z - 2000.0);
    let mut s = String::new();
    for (label, flags) in [("map", 1), ("all", 511)] {
        match n::raycast(from, to, flags, ignore) {
            Some(h) => {
                let _ = write!(
                    s,
                    "{label}: hit z={:.2} (dz {:+.1}) nz={:.2} mat=0x{:08X} ent={}  ",
                    h.pos.z,
                    h.pos.z - at.z,
                    h.normal.z,
                    h.material,
                    h.entity
                );
            }
            None => {
                let _ = write!(s, "{label}: no hit  ");
            }
        }
    }
    match n::get_ground_z(Vec3::new(at.x, at.y, at.z + 50.0)) {
        Some(z) => {
            let _ = write!(s, "groundZ={z:.2}");
        }
        None => s.push_str("groundZ=none"),
    }
    s
}

struct State {
    overlay: bool,
    next_dest: usize,
    held: Option<i32>,
    status: String,
    last_log: i32,
}

/// Teleports, holds the entity frozen while collision streams, then probes.
fn teleport(state: &mut State, dest: &Destination) {
    let ped = n::player_ped_id();
    let ent = controlled_entity(ped);
    log(&format!("TELEPORT {} -> ({:.1}, {:.1}, {:.1}) entity {ent}", dest.name, dest.pos.x, dest.pos.y, dest.pos.z));
    n::freeze_entity_position(ent, true);
    n::set_entity_coords(ent, dest.pos);
    n::set_entity_velocity(ent, 0.0, 0.0, 0.0);

    let start = n::get_game_timer();
    let mut loaded = false;
    while n::get_game_timer() - start < 10_000 {
        n::request_collision_at_coord(dest.pos);
        if n::has_collision_loaded_around_entity(ent) {
            loaded = true;
            break;
        }
        n::draw_text(&format!("paradise: loading collision at {}...", dest.name), 0.4, 0.45, 0.5, [255, 220, 80, 255]);
        shv::wait(0);
    }
    let waited = n::get_game_timer() - start;
    // Give streaming a few more frames even after "loaded" reports true.
    for _ in 0..30 {
        n::request_collision_at_coord(dest.pos);
        shv::wait(0);
    }
    let probe = describe_probe(dest.pos, ent);
    log(&format!("  collision_loaded={loaded} after {waited} ms; {probe}"));

    match n::raycast(
        Vec3::new(dest.pos.x, dest.pos.y, dest.pos.z + 50.0),
        Vec3::new(dest.pos.x, dest.pos.y, dest.pos.z - 2000.0),
        1,
        ent,
    ) {
        Some(h) if (h.pos.z - dest.pos.z).abs() < 60.0 => {
            n::set_entity_coords(ent, Vec3::new(dest.pos.x, dest.pos.y, h.pos.z + 1.0));
            n::freeze_entity_position(ent, false);
            state.held = None;
            state.status = format!("{}: ground at z={:.2}, released", dest.name, h.pos.z);
        }
        _ => {
            state.held = Some(ent);
            state.status = format!("{}: NO GROUND near target - holding (F5 to release)", dest.name);
        }
    }
    log(&format!("  result: {}", state.status));
}

extern "C" fn script_main() {
    log("paradise.asi started");
    let mut keys = Keys([false; 256]);
    let mut state = State { overlay: true, next_dest: 0, held: None, status: "F6: teleport test, F7: overlay".into(), last_log: 0 };

    let overlay = overlay::Overlay::load("paradise_col.bin");
    match &overlay {
        Some(o) => log(&format!("overlay loaded: {} triangles", o.triangle_count())),
        None => log("overlay: paradise_col.bin not found or invalid"),
    }
    let mut show_overlay = false;
    let mut trace = std::fs::File::create("paradise_trace.csv").ok();
    if let Some(f) = trace.as_mut() {
        let _ = writeln!(f, "t,x,y,z,vx,vy,vz,speed,air");
    }
    let mut last_trace = 0;

    // Wait for the loading screen to finish, then apply story-mode map DLC.
    // In Paradise mode the screen is kept black until the player stands in
    // Paradise City: the game first plays the character's arrival scene
    // (e.g. pull-ups at Vespucci Beach) in the removed Los Santos.
    let settings = load_settings();
    while n::is_loading_screen_active() {
        if settings.paradise {
            n::do_screen_fade_out(0);
        }
        shv::wait(100);
    }
    if settings.paradise {
        let t = std::time::Instant::now();
        while t.elapsed().as_millis() < 3000 {
            n::do_screen_fade_out(0);
            end_arrival_scene();
            shv::wait(0);
        }
    } else {
        shv::wait(3000);
    }
    if !settings.paradise {
        log("@mode off: Paradise map not applied");
        state.status = "Paradise mode off (paradise.ini). Normal GTA.".into();
    } else {
        let start = settings.start.map(|pos| Destination { name: "start".into(), pos });
        if let Some(d) = &start {
            // Hold the player at the Paradise start before the swap.
            let ent = controlled_entity(n::player_ped_id());
            n::freeze_entity_position(ent, true);
            n::set_entity_coords(ent, d.pos);
        }
        n::on_enter_sp();
        log("ON_ENTER_SP called (auto)");
        shv::wait(500);
        while n::is_loading_screen_active() {
            shv::wait(250);
        }
        if let Some(d) = &start {
            teleport(&mut state, d);
        }
        if let Some(d) = &start {
            base_map_check(d.pos);
        }
        end_arrival_scene();
        n::do_screen_fade_in(800);
        state.status = "Paradise City applied. F6: teleport".into();
    }
    let rescue_to = settings.paradise.then_some(()).and(settings.start).map(|pos| Destination { name: "start (rescue)".into(), pos });
    let mut rescue = Rescue::default();
    let mut crash = wreck::Wreck::default();
    let mut wreck_settings = settings.wreck;
    let mut boost_settings = settings.boost;
    let mut air_gravity = settings.air_gravity;
    let mut downforce = settings.downforce;
    let mut police = settings.police;
    let mut debug_log = settings.debug_log;
    let mut perf_on = settings.perf;
    let mut perf = perf::Perf::new(n::get_game_timer());
    let mut boost = boost::Boost::default();
    let mut chase = chase::Chase::new();
    let mut shield = propshield::PropShield::load();
    let mut guard = settings.paradise.then(spawnguard::SpawnGuard::load);
    let mut last_settings = 0;

    loop {
        if keys.pressed(VK_F4) {
            show_overlay = !show_overlay;
            log(&format!("overlay {}", if show_overlay { "on" } else { "off" }));
        }
        if keys.pressed(VK_F10) {
            n::on_enter_sp();
            log("ON_ENTER_SP called (F10)");
            state.status = "ON_ENTER_SP called".into();
        }
        if keys.pressed(VK_F7) {
            state.overlay = !state.overlay;
        }
        if keys.pressed(VK_F5) {
            if let Some(ent) = state.held.take() {
                n::freeze_entity_position(ent, false);
                state.status = "released".into();
                log("RELEASE");
            }
        }
        if keys.pressed(VK_F6) {
            let dests = load_destinations();
            if dests.is_empty() {
                state.status = "paradise.ini has no destinations".into();
            } else {
                let d = &dests[state.next_dest % dests.len()];
                state.next_dest = (state.next_dest + 1) % dests.len();
                teleport(&mut state, d);
                crash.reset();
            }
        }

        let ped = n::player_ped_id();
        let ent = controlled_entity(ped);
        let pos = n::get_entity_coords(ent);
        let now = n::get_game_timer();
        perf.start();
        if let Some(g) = guard.as_mut() {
            g.update(ped, now);
        }
        perf.done(0);
        if let Some(d) = &rescue_to {
            if let Some(why) = rescue.check(ped, ent, pos, now) {
                if let RescueKind::Respawned(_, true) = why {
                    // GTA starts its hospital / police station exit scene
                    // just after the respawn; stop it behind the black.
                    let t = std::time::Instant::now();
                    while t.elapsed().as_millis() < 1000 {
                        n::do_screen_fade_out(0);
                        end_arrival_scene();
                        shv::wait(0);
                    }
                }
                let near = match why {
                    RescueKind::Respawned(Some(at), _) => respawn_near(at),
                    _ => None,
                };
                let what = match why {
                    RescueKind::Respawned(..) => "respawned",
                    RescueKind::NoGround => "no ground below",
                };
                log(&format!("RESCUE ({what}) at ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z));
                teleport(&mut state, near.as_ref().unwrap_or(d));
                if matches!(why, RescueKind::Respawned(_, true)) {
                    n::do_screen_fade_in(800);
                }
                rescue = Rescue::default();
                crash.reset();
                continue;
            }
        }
        if now - last_settings > 3000 {
            last_settings = now;
            let s = load_settings();
            wreck_settings = s.wreck;
            boost_settings = s.boost;
            air_gravity = s.air_gravity;
            downforce = s.downforce;
            police = s.police;
            debug_log = s.debug_log;
            perf_on = s.perf;
        }
        if state.held.is_none() {
            let driving = if n::is_ped_in_any_vehicle(ped) { n::get_vehicle_ped_is_in(ped) } else { 0 };
            let driven = if driving != 0 && n::get_ped_in_vehicle_seat(driving, -1) == ped { driving } else { 0 };
            perf.start();
            shield.sweep(driven, now);
            perf.done(1);
            shield.update(driven);
            perf.done(2);
            let event = crash.update(&wreck_settings, ped, now, &mut shield);
            if matches!(event, wreck::Event::Wrecked { .. } | wreck::Event::Finished(_)) {
                // The wreck scene runs inside that call; it isn't frame cost.
                perf.start();
            } else {
                perf.done(3);
            }
            match event {
                wreck::Event::Wrecked { dv } => {
                    boost.stop();
                    log(&format!("WRECKED at ({:.1}, {:.1}, {:.1}), speed change {:.1} m/s", pos.x, pos.y, pos.z, dv));
                    rescue = Rescue::default();
                    continue;
                }
                wreck::Event::Finished(why) => {
                    boost.stop();
                    log(&format!("WRECKED ({why}) at ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z));
                    rescue = Rescue::default();
                    continue;
                }
                wreck::Event::Prop(dv) => log(&format!("prop knocked over ({dv:.1} m/s), speed and health given back")),
                wreck::Event::Impact(dv) => log(&format!("impact {dv:.1} m/s")),
                wreck::Event::None => {}
            }
            perf.start();
            boost.update(&boost_settings, ped, now);
            perf.done(4);
            physics::vehicle_forces(air_gravity, downforce, pos);
            perf.done(5);
            chase.update(&police, ped, pos, now);
            perf.done(6);
        }
        if perf_on {
            perf.frame(now, n::get_frame_time());
        }
        if keys.pressed(VK_F9) {
            let n_mark = mark_spot(pos, n::get_entity_heading(ent), n::is_ped_in_any_vehicle(ped));
            log(&format!("MARK {n_mark} at ({:.2}, {:.2}, {:.2})", pos.x, pos.y, pos.z));
            state.status = format!("mark {n_mark} saved ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z);
        }
        if debug_log && now - state.last_log > 2000 {
            state.last_log = now;
            log(&format!(
                "pos ({:.1}, {:.1}, {:.1}) air={} col_loaded={} {}",
                pos.x,
                pos.y,
                pos.z,
                n::is_entity_in_air(ent),
                n::has_collision_loaded_around_entity(ent),
                describe_probe(pos, ent)
            ));
        }
        if show_overlay {
            if let Some(o) = &overlay {
                o.draw(pos, 70.0, 35.0, 1400);
            }
        }
        if debug_log && n::is_ped_in_any_vehicle(ped) && now - last_trace >= 50 {
            last_trace = now;
            if let Some(f) = trace.as_mut() {
                let v = n::get_entity_velocity(ent);
                let _ = writeln!(
                    f,
                    "{:.3},{:.2},{:.2},{:.3},{:.3},{:.3},{:.3},{:.2},{}",
                    now as f32 / 1000.0,
                    pos.x,
                    pos.y,
                    pos.z,
                    v.x,
                    v.y,
                    v.z,
                    n::get_entity_speed(ent),
                    n::is_entity_in_air(ent) as u8
                );
            }
        }
        if state.overlay {
            let lines = [
                format!("paradise  pos {:.1}, {:.1}, {:.1}  hdg {:.0}", pos.x, pos.y, pos.z, n::get_entity_heading(ent)),
                state.status.clone(),
                format!("F6 teleport  F4 collision overlay ({})  F9 mark spot  F7 hide text", if show_overlay { "on" } else { "off" }),
            ];
            for (i, l) in lines.iter().enumerate() {
                n::draw_text(l, 0.01, 0.01 + i as f32 * 0.025, 0.35, [255, 255, 255, 230]);
            }
        }
        shv::wait(0);
    }
}
