//! Character switching (`@switch`, Paradise City only). GTA's own wheel,
//! switch and story script do the switch, and the script makes the player
//! the new character; the plugin only makes it land in Paradise City.
//!
//! GTA's scripts make the character (and often their car) at a place of
//! their own in the removed Los Santos, now under, inside or above Paradise
//! City, with an arrival scene (Michael walking out of a shop, ...). The
//! spawn guard leaves them alone (spawnguard.rs), and:
//!
//! - At the hold (state 8), the character is the one with a protagonist's
//!   model (not the player's) nearest the camera, which is over GTA's place;
//!   looked for every frame until they're there (up to HOLD_WAIT_MS).
//!   Already driving on a Paradise road: left as they are. Otherwise they're
//!   put in a new car of their own, driving, on the Paradise road nearest
//!   GTA's place, however far (GTA's place can be well outside the city).
//!   GTA's own car for them is deleted: its arrival scene kept pulling it
//!   back to GTA's place. The car loads collision around itself and is held
//!   still until it has.
//! - Then GTA's descent and final swoop are allowed, as the script would once
//!   its scene is ready (ALLOW_PLAYER_SWITCH_DESCENT / _OUTRO).
//! - GTA's arrival scene takes the character out of the car to its own place;
//!   they're put back in (during the switch, and for SCENE_GUARD_MS after it
//!   when the scene moved them more than SCENE_AWAY from the car). A car
//!   moved off its road during the switch is put back too. Once the player
//!   is the new character, every script that started during the switch is
//!   stopped, with its dialogue: the arrival scene (player_timetable_scene,
//!   player_scene_*) and whatever GTA started near its place (drunk, street
//!   characters, shops, ...). No arrival scenes or events.
//! - The character left behind drives on if they were driving. A car the
//!   plugin made is given back to the game (no longer needed) at the end.
//! - Short switches (type 3, characters close together) are left to GTA.
//!
//! Safety nets: if the player is still the old character END_GRACE_MS after
//! the switch, the plugin makes them the new one itself and lets the old one
//! go; a switch still running after GIVE_UP_MS is stopped. Either way GTA's
//! arrival-scene scripts still running are stopped (they keep the controls
//! and radar off) and the camera, radar and control given back.
//!
//! Logged: each switch, its states, the scripts that start during it, the
//! characters at the hold and which one it's going to, the car, put-backs,
//! and how it ended.

use crate::gps::GpsWatch;
use crate::natives as n;
use crate::propshield::joaat;
use crate::shv::{self, Vec3};

/// Their car's model loads within this long of the hold; a switch takes at
/// most GIVE_UP_MS.
const LOAD_MS: i32 = 5000;
const GIVE_UP_MS: i32 = 30000;
/// The switch state that holds before the descent, and how long to wait
/// there for the character and collision under their car before letting GTA
/// go on anyway.
const HOLD: i32 = 8;
const HOLD_WAIT_MS: i32 = 3000;
/// GTA's short switch (characters close together): left to GTA.
const SHORT: i32 = 3;
/// A character driving this near (m) a Paradise road is left as they are.
const ON_ROAD: f32 = 15.0;
/// GTA's own car for a character on foot (deleted): their model, made by a
/// script, within this far (m).
const OWN_CAR_NEAR: f32 = 50.0;
/// After the switch: how long GTA's script gets to make the player the new
/// character, and how long its arrival scene is watched.
const END_GRACE_MS: i32 = 1000;
const SCENE_GUARD_MS: i32 = 5000;
/// A character out of their car this far (m) from it after the switch was
/// moved by the scene (nearer, they got out themselves); a car this far from
/// its road during the switch was moved off it.
const SCENE_AWAY: f32 = 10.0;
const MAX_PUT_BACK: u32 = 30;
/// TASK_VEHICLE_DRIVE_WANDER speed (m/s) and driving style (normal traffic).
const WANDER_SPEED: f32 = 18.0;
const WANDER_STYLE: i32 = 786603;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Who {
    Michael,
    Franklin,
    Trevor,
}

impl Who {
    const ALL: [Who; 3] = [Who::Michael, Who::Franklin, Who::Trevor];

    fn name(self) -> &'static str {
        match self {
            Who::Michael => "Michael",
            Who::Franklin => "Franklin",
            Who::Trevor => "Trevor",
        }
    }

    fn model(self) -> u32 {
        joaat(match self {
            Who::Michael => "player_zero",
            Who::Franklin => "player_one",
            Who::Trevor => "player_two",
        })
    }

    /// Their own car: Michael's Tailgater, Franklin's Buffalo, Trevor's Bodhi.
    fn car(self) -> u32 {
        joaat(match self {
            Who::Michael => "tailgater",
            Who::Franklin => "buffalo2",
            Who::Trevor => "bodhi2",
        })
    }

    fn of(model: u32) -> Option<Who> {
        Who::ALL.into_iter().find(|w| w.model() == model)
    }
}

fn name_of(who: Option<Who>) -> &'static str {
    who.map_or("someone else", Who::name)
}

/// The character a switch is going to, the road they'll drive (None: left as
/// GTA has them) and GTA's own car for them (0: none), deleted once they're
/// in a new one.
#[derive(Clone, Copy)]
struct Target {
    ped: i32,
    who: Who,
    road: Option<(Vec3, f32)>,
    scene_car: i32,
}

/// A switch GTA is running (or has just finished).
struct Running {
    since: i32,
    kind: i32,
    /// Who the player was when it started, and their ped.
    from: Option<Who>,
    from_ped: i32,
    /// Scripts running when it started, and so far (the log names the ones
    /// that start).
    before: Vec<String>,
    scripts: Vec<String>,
    last_state: i32,
    hold_since: Option<i32>,
    /// Looked for at the hold.
    target: Option<Target>,
    looked: bool,
    /// Their car (0: not yet, -1: none will be), whether the plugin made it,
    /// and whether collision has loaded under it.
    car: i32,
    ours: bool,
    landed: bool,
    allowed: bool,
    put_back: u32,
    car_back: bool,
    /// After the switch: when it ended, whether the player is the new
    /// character, whether they got out of the car themselves.
    ended: Option<i32>,
    done: bool,
    got_out: bool,
}

impl Running {
    fn log_new_scripts(&mut self) {
        let new: Vec<String> = running_scripts().into_iter().filter(|s| !self.before.contains(s) && !self.scripts.contains(s)).collect();
        if !new.is_empty() {
            crate::log(&format!("switch: scripts started: {}", new.join(", ")));
            self.scripts.extend(new);
        }
    }

    /// The car back to the game if the plugin made it, and the car models.
    fn finish(&self) {
        if self.car > 0 {
            if !self.landed {
                n::freeze_entity_position(self.car, false);
            }
            if self.ours {
                n::set_vehicle_as_no_longer_needed(self.car);
            }
        }
        for w in Who::ALL {
            if Some(w) != self.from {
                n::set_model_as_no_longer_needed(w.car());
            }
        }
        if self.put_back > 0 {
            crate::log(&format!("switch: put back in the car {} time(s) in all", self.put_back));
        }
    }
}

#[derive(Default)]
pub struct Switch {
    running: Option<Running>,
}

/// The scripts started since `before` (GTA's arrival scene and whatever it
/// started near its place) stopped with their dialogue, then camera, radar
/// and control back to the player.
fn give_back(before: &[String]) -> String {
    let mut stopped = Vec::new();
    n::script_thread_iterator_reset();
    loop {
        let id = n::script_thread_next();
        if id == 0 {
            break;
        }
        if let Some(name) = n::script_name(id) {
            if name == "player_timetable_scene" || name.starts_with("player_scene_") || !before.contains(&name) {
                n::terminate_thread(id);
                stopped.push(name);
            }
        }
    }
    n::stop_scripted_conversation();
    if stopped.iter().any(|s| s.contains("drunk")) {
        n::sober_up(n::player_ped_id());
    }
    n::set_player_control(true);
    n::display_radar(true);
    n::render_script_cams(false, 0);
    n::destroy_all_cams();
    if stopped.is_empty() {
        "camera, radar and control given back".to_string()
    } else {
        format!("stopped {}; camera, radar and control given back", stopped.join(", "))
    }
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn dist3(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

fn running_scripts() -> Vec<String> {
    let mut v = Vec::new();
    n::script_thread_iterator_reset();
    loop {
        let id = n::script_thread_next();
        if id == 0 {
            break;
        }
        if let Some(name) = n::script_name(id) {
            v.push(name);
        }
    }
    v
}

impl Switch {
    /// A switch is under way: the player's ped and car are about to change.
    pub fn busy(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.ended.is_none() || !r.done)
    }

    pub fn update(&mut self, on: bool, gps: &mut GpsWatch, now: i32) {
        let in_progress = n::is_player_switch_in_progress();
        let me = n::player_ped_id();
        let Some(mut r) = self.running.take() else {
            if in_progress && on {
                let from = Who::of(n::get_entity_model(me));
                let kind = n::get_player_switch_type();
                crate::log(&format!(
                    "switch: GTA's switch started from {} (type {kind}, state {}){}",
                    name_of(from),
                    n::get_player_switch_state(),
                    if kind == SHORT { "; a short one, left to GTA" } else { "" }
                ));
                for w in Who::ALL {
                    if Some(w) != from {
                        n::request_model(w.car());
                    }
                }
                self.running = Some(Running {
                    since: now,
                    kind,
                    from,
                    from_ped: me,
                    before: running_scripts(),
                    scripts: Vec::new(),
                    last_state: -1,
                    hold_since: None,
                    target: None,
                    looked: false,
                    car: 0,
                    ours: false,
                    landed: false,
                    allowed: false,
                    put_back: 0,
                    car_back: false,
                    ended: None,
                    done: false,
                    got_out: false,
                });
            }
            return;
        };
        let ms = now - r.since;
        if r.ended.is_some() && r.done && in_progress {
            // Another switch already: picked up next frame.
            r.finish();
            return;
        }
        if r.ended.is_some() || !in_progress {
            if after(&mut r, me, now) {
                self.running = Some(r);
            }
            return;
        }
        let state = n::get_player_switch_state();
        if state != r.last_state {
            crate::log(&format!("switch: state {state} at {ms} ms"));
            r.last_state = state;
            r.log_new_scripts();
        }
        if ms > GIVE_UP_MS {
            n::stop_player_switch();
            let back = give_back(&r.before);
            r.finish();
            crate::log(&format!("switch: not finished in {GIVE_UP_MS} ms (state {state}): stopped; {back}"));
            return;
        }
        if r.kind == SHORT || state < HOLD {
            self.running = Some(r);
            return;
        }
        let held = now - *r.hold_since.get_or_insert(now);
        if !r.looked {
            r.target = find_target(me, gps);
            if r.target.is_some() {
                r.looked = true;
            } else if held > HOLD_WAIT_MS {
                r.looked = true;
                let cam = n::get_final_rendered_cam_coord();
                crate::log(&format!("switch: no other character in the world {held} ms into the hold (camera over ({:.0}, {:.0}))", cam.x, cam.y));
            }
        }
        match r.target {
            Some(t) => {
                if r.car == 0 {
                    match t.road {
                        None => r.car = -1,
                        Some((at, heading)) => {
                            if n::has_model_loaded(t.who.car()) {
                                let car = n::create_vehicle(t.who.car(), Vec3::new(at.x, at.y, at.z + 0.5), heading);
                                n::set_model_as_no_longer_needed(t.who.car());
                                if car == 0 {
                                    crate::log(&format!("switch: {}'s car wasn't created; left where GTA put them", t.who.name()));
                                    r.car = -1;
                                } else {
                                    n::set_entity_as_mission_entity(car);
                                    r.ours = true;
                                    r.car = put_in_car(t, car, at, heading, ms, "a new car");
                                    drop_scene_car(t, ms);
                                }
                            } else if held > LOAD_MS {
                                crate::log(&format!("switch: {}'s car didn't load in {LOAD_MS} ms; left where GTA put them", t.who.name()));
                                r.car = -1;
                            } else {
                                n::request_model(t.who.car());
                            }
                        }
                    }
                }
                if r.car > 0 && !r.landed {
                    n::request_collision_at_coord(n::get_entity_coords(r.car));
                    if n::has_collision_loaded_around_entity(r.car) {
                        n::freeze_entity_position(r.car, false);
                        r.landed = true;
                        crate::log(&format!("switch: collision loaded under {}'s car at {ms} ms", t.who.name()));
                    }
                }
                keep_car_on_road(&mut r, t, ms);
                keep_in_car(&mut r, t, ms);
            }
            None if r.looked => r.car = -1,
            None => {}
        }
        if !r.allowed && r.looked && (r.landed || r.car < 0 || held > HOLD_WAIT_MS) {
            let ready = n::is_switch_ready_for_descent();
            n::allow_player_switch_descent();
            n::allow_player_switch_outro();
            r.allowed = true;
            crate::log(&format!("switch: GTA's descent and swoop allowed at {ms} ms ({held} ms into the hold, ground loaded: {}, GTA ready: {ready})", r.landed));
        }
        self.running = Some(r);
    }
}

/// After GTA's switch: who the player is now (the safety net if GTA's script
/// didn't change them), then GTA's arrival scene for a while. Whether to keep
/// watching.
fn after(r: &mut Running, me: i32, now: i32) -> bool {
    let ended = *r.ended.get_or_insert(now);
    let since_end = now - ended;
    let ms = ended - r.since;
    let now_who = Who::of(n::get_entity_model(me));
    if since_end == 0 {
        r.log_new_scripts();
    }
    let Some(t) = r.target.filter(|_| r.kind != SHORT) else {
        if r.kind != SHORT && now_who == r.from {
            let back = give_back(&r.before);
            crate::log(&format!("switch: ended after {ms} ms with the player still {}: {back}", name_of(now_who)));
        } else {
            crate::log(&format!("switch: ended after {ms} ms; the player is {}", name_of(now_who)));
        }
        r.finish();
        return false;
    };
    if !r.done {
        if now_who == r.from && me != t.ped {
            if since_end < END_GRACE_MS {
                return true;
            }
            n::change_player_ped(t.ped);
            let back = give_back(&r.before);
            if r.car > 0 {
                n::clear_ped_tasks(t.ped);
            }
            let left = leave_behind(r.from_ped, t.ped);
            n::set_entity_as_no_longer_needed(r.from_ped);
            crate::log(&format!(
                "switch: GTA's script hadn't made the player {} {END_GRACE_MS} ms after the switch ({ms} ms); the plugin did, {back}; {} left {left} and let go",
                t.who.name(),
                name_of(r.from)
            ));
        } else {
            // GTA's arrival scene and its dialogue: not wanted, stopped now
            // the player is the new character.
            let back = give_back(&r.before);
            n::clear_ped_tasks(me);
            let left = leave_behind(r.from_ped, me);
            crate::log(&format!(
                "switch: done after {ms} ms; the player is now {}{}; {back}; {} left {left}",
                name_of(now_who),
                if me == t.ped { "" } else { " (not the ped the plugin chose)" },
                name_of(r.from)
            ));
        }
        r.done = true;
    }
    if since_end < SCENE_GUARD_MS && !r.got_out {
        keep_in_car(r, t, now - r.since);
        return true;
    }
    r.log_new_scripts();
    r.finish();
    false
}

/// The character left behind drives on if they were driving.
fn leave_behind(old: i32, player: i32) -> &'static str {
    if old == player || !n::does_entity_exist(old) {
        return "gone";
    }
    if !n::is_ped_in_any_vehicle(old) {
        return "on foot";
    }
    let car = n::get_vehicle_ped_is_in(old);
    if n::get_ped_in_vehicle_seat(car, -1) != old {
        return "in a vehicle";
    }
    n::task_vehicle_drive_wander(old, car, WANDER_SPEED, WANDER_STYLE);
    "driving on"
}

/// During the switch, a car moved off its road (GTA's scripts placing their
/// own car) goes back on it, once.
fn keep_car_on_road(r: &mut Running, t: Target, ms: i32) {
    let Some((at, heading)) = t.road else { return };
    if r.car <= 0 || r.car_back || !n::does_entity_exist(r.car) {
        return;
    }
    let c = n::get_entity_coords(r.car);
    let away = dist3(c, Vec3::new(at.x, at.y, at.z + 0.5));
    if away < SCENE_AWAY {
        return;
    }
    r.car_back = true;
    n::set_entity_coords(r.car, Vec3::new(at.x, at.y, at.z + 0.5));
    n::set_entity_heading(r.car, heading);
    crate::log(&format!(
        "switch: {}'s car was moved off the road at {ms} ms to ({:.0}, {:.0}, {:.0}), {away:.0} m: put back",
        t.who.name(),
        c.x,
        c.y,
        c.z
    ));
}

/// GTA's arrival scene takes the character out of the car to its own place:
/// they're put back in. After the switch, only when they're far from the car
/// (nearer, they got out themselves, and are left alone from then on).
fn keep_in_car(r: &mut Running, t: Target, ms: i32) {
    if r.car <= 0 || r.put_back >= MAX_PUT_BACK || !n::does_entity_exist(r.car) || !n::does_entity_exist(t.ped) {
        return;
    }
    if n::is_ped_in_any_vehicle(t.ped) && n::get_vehicle_ped_is_in(t.ped) == r.car {
        return;
    }
    let p = n::get_entity_coords(t.ped);
    let away = dist3(p, n::get_entity_coords(r.car));
    if r.ended.is_some() && away < SCENE_AWAY {
        r.got_out = true;
        crate::log(&format!("switch: {} got out of their car at {ms} ms; left alone", t.who.name()));
        return;
    }
    n::clear_ped_tasks_immediately(t.ped);
    n::set_ped_into_vehicle(t.ped, r.car);
    if t.ped != n::player_ped_id() {
        n::task_vehicle_drive_wander(t.ped, r.car, WANDER_SPEED, WANDER_STYLE);
    }
    r.put_back += 1;
    if r.put_back == 1 {
        crate::log(&format!(
            "switch: {} was taken out of their car at {ms} ms ({}) to ({:.0}, {:.0}, {:.0}), {away:.0} m from it: put back",
            t.who.name(),
            if r.ended.is_some() { "after the switch".to_string() } else { format!("state {}", r.last_state) },
            p.x,
            p.y,
            p.z
        ));
        r.log_new_scripts();
    }
}

/// GTA's own car for them deleted, once they're out of it.
fn drop_scene_car(t: Target, ms: i32) {
    let v = t.scene_car;
    if v == 0 || !n::does_entity_exist(v) || n::get_ped_in_vehicle_seat(v, -1) != 0 {
        return;
    }
    n::delete_vehicle(v);
    crate::log(&format!("switch: GTA's own car for {} deleted at {ms} ms", t.who.name()));
}

/// GTA's own car for a character on foot: their model, made by a script,
/// with no driver, nearest them within OWN_CAR_NEAR.
fn own_car_near(who: Who, p: Vec3) -> Option<i32> {
    shv::all_vehicles()
        .into_iter()
        .filter(|&v| n::does_entity_exist(v) && n::get_entity_model(v) == who.car() && n::get_ped_in_vehicle_seat(v, -1) == 0 && n::get_entity_script(v).is_some())
        .map(|v| (v, dist3(n::get_entity_coords(v), p)))
        .filter(|&(_, d)| d < OWN_CAR_NEAR)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(v, _)| v)
}

/// The character the switch is going to: the one nearest the camera, which
/// is over GTA's place for them at the hold (None: none in the world yet).
fn find_target(me: i32, gps: &mut GpsWatch) -> Option<Target> {
    let mine = n::get_entity_model(me);
    let cam = n::get_final_rendered_cam_coord();
    let peds: Vec<(i32, Who, f32)> = shv::all_peds()
        .into_iter()
        .filter(|&p| p != me && n::does_entity_exist(p) && n::get_entity_model(p) != mine)
        .filter_map(|p| Some((p, Who::of(n::get_entity_model(p))?, dist(n::get_entity_coords(p), cam))))
        .collect();
    let &(ped, who, _) = peds.iter().min_by(|a, b| a.2.total_cmp(&b.2))?;
    let list = peds.iter().map(|&(_, w, d)| format!("{} {d:.0} m", w.name())).collect::<Vec<_>>().join(", ");
    let was = n::get_entity_coords(ped);
    let driving = if n::is_ped_in_any_vehicle(ped) {
        let v = n::get_vehicle_ped_is_in(ped);
        (n::get_ped_in_vehicle_seat(v, -1) == ped).then_some(v)
    } else {
        None
    };
    let scene_car = driving.or_else(|| own_car_near(who, was)).unwrap_or(0);
    let near = gps.nearest_road(was);
    let road = near.or_else(|| gps.nearest_road_any(was));
    let on_road = driving.is_some() && road.is_some_and(|(at, _)| dist3(at, was) < ON_ROAD);
    let plan = match road {
        _ if on_road => "already driving on a road, left as GTA has them".to_string(),
        Some((at, heading)) => format!(
            "putting them in a new car on the {} at ({:.0}, {:.0}, {:.0}) heading {heading:.0}, {:.0} m from there{}",
            if near.is_some() { "road nearest it" } else { "nearest road anywhere" },
            at.x,
            at.y,
            at.z,
            dist3(at, was),
            if scene_car != 0 { "; GTA's own car for them goes" } else { "" }
        ),
        None => "no roads loaded, left there".to_string(),
    };
    crate::log(&format!(
        "switch: at the hold the camera is over ({:.0}, {:.0}); characters: {list}; going to {}: GTA has them at ({:.0}, {:.0}, {:.0}) (made by {}); {plan}",
        cam.x,
        cam.y,
        who.name(),
        was.x,
        was.y,
        was.z,
        n::get_entity_script(ped).unwrap_or_else(|| "no script".to_string())
    ));
    Some(Target { ped, who, road: if on_road { None } else { road }, scene_car })
}

/// The character in their car at their road, driving off as traffic does;
/// the car loads collision around itself and is held where it is until that
/// collision is in.
fn put_in_car(t: Target, car: i32, at: Vec3, heading: f32, ms: i32, whose: &str) -> i32 {
    n::set_entity_coords(car, Vec3::new(at.x, at.y, at.z + 0.5));
    n::set_entity_heading(car, heading);
    n::set_entity_load_collision_flag(car, true);
    n::freeze_entity_position(car, true);
    n::request_collision_at_coord(at);
    n::set_vehicle_engine_on(car);
    if n::get_vehicle_ped_is_in(t.ped) != car || !n::is_ped_in_any_vehicle(t.ped) {
        n::clear_ped_tasks_immediately(t.ped);
        n::set_ped_into_vehicle(t.ped, car);
    }
    n::task_vehicle_drive_wander(t.ped, car, WANDER_SPEED, WANDER_STYLE);
    crate::log(&format!("switch: {} in {whose} at {ms} ms (in it: {})", t.who.name(), n::get_vehicle_ped_is_in(t.ped) == car));
    car
}
