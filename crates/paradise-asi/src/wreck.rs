//! Burnout-style crashes for the player's car.
//!
//! Every frame the car's velocity is compared with ~0.15 s earlier. A sudden
//! horizontal change (hitting something, not braking or landing) is an
//! impact:
//! - above `@impact_speed`: extra dents where the car was hit, plus broken
//!   glass and panels on harder hits;
//! - above `@wreck_speed`: a wreck. The car is smashed up and blown apart in
//!   slow motion ("WRECKED"), then the screen fades and the player comes
//!   back on the nearest road, in a fresh copy of the same car (colours,
//!   upgrades and plate kept), already rolling.
//!
//! Burnout's crash camera starts just before the impact, but only when the
//! wreck is all but certain (otherwise it starts at the impact): lines
//! traced straight ahead along the car's path, within its width, must find
//! a wall or vehicle within `@precrash_lead` s, nearly head-on and well over
//! the wreck threshold, on at least two of three lines (`@precrash`).
//! Steering clear during the run-up, or a hit that isn't a wreck after all,
//! brings normal speed back.
//!
//! A car that is finished in other ways is wrecked too (`@wreck_damage`): on
//! fire, engine dead, no longer drivable, stuck on its roof, or not moving
//! with the accelerator held (wheels jammed) for `@wreck_stuck` seconds.
//!
//! Knocking over a prop (lamp post, bin, sign...) is not a crash: the car
//! keeps `@prop_keep` of its speed and the hit costs it no health (the prop
//! shield, propshield.rs, already stops it denting the car).
//!
//! Once the screen has faded after a wreck, NPCs and vehicles within
//! CLEAR_RADIUS of the wreck are removed, so the respawn starts clean.
//!
//! The player is kept invincible through a wreck, so GTA's own death and
//! hospital respawn never happen for one; the wreck is the death.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::VecDeque;

/// `@...` wreck settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct WreckSettings {
    pub enabled: bool,
    /// Sudden change in speed (m/s) for a wreck; 18 m/s is about 65 km/h.
    pub wreck_dv: f32,
    /// Sudden change in speed (m/s) for extra crash damage.
    pub impact_dv: f32,
    /// Blow the car up on a wreck.
    pub explode: bool,
    /// Speed (m/s) the new car starts at.
    pub respawn_speed: f32,
    /// Slow-motion crash time (real seconds) before the fade.
    pub crash_time: f32,
    /// Game speed during the crash (1 = normal).
    pub slowmo: f32,
    /// Show the crash from an orbiting camera.
    pub crash_cam: bool,
    /// Strength of each crush round on a wreck.
    pub crush: f32,
    /// Crush rounds over the start of the crash.
    pub crush_rounds: i32,
    /// Wreck a car that is on fire, dead or jammed.
    pub damage_wreck: bool,
    /// Seconds a jammed car (accelerator held, not moving) is given.
    pub stuck_time: f32,
    /// Fraction of its speed a car keeps when it knocks over a prop.
    pub prop_keep: f32,
    /// Start the slow-motion crash camera just before a predicted wreck.
    pub precrash: bool,
    /// How far ahead (game seconds) a wreck is predicted.
    pub precrash_lead: f32,
    /// NPCs and vehicles this close (m) to the wreck are removed before the
    /// respawn (0: off).
    pub clear_radius: f32,
}

impl Default for WreckSettings {
    fn default() -> Self {
        Self { enabled: true, wreck_dv: 18.0, impact_dv: 11.0, explode: true, respawn_speed: 12.0, crash_time: 5.0, slowmo: 0.15, crash_cam: true, crush: 8000.0, crush_rounds: 8, damage_wreck: true, stuck_time: 2.5, prop_keep: 0.9, precrash: true, precrash_lead: 0.12, clear_radius: 6.0 }
    }
}

/// How far back the velocity is compared (ms).
const WINDOW_MS: i32 = 150;
/// No new impact this soon after the last (ms).
const COOLDOWN_MS: i32 = 400;
/// A speed change this sudden (m/s) is checked for a prop being hit.
const PROP_DV: f32 = 2.5;
/// INPUT_VEH_ACCELERATE
const CONTROL_ACCELERATE: i32 = 71;
/// Front of a car from its centre (m), where the look-ahead lines start
/// counting distance; and the corner lines' offset to each side.
const NOSE: f32 = 2.4;
const CORNER: f32 = 0.5;
/// A predicted hit must be this much harder than a wreck...
const PRE_MARGIN: f32 = 1.25;
/// ...and this close to head-on (share of the speed going into the surface).
const PRE_HEAD_ON: f32 = 0.7;
/// A vehicle hit takes roughly this share of the closing speed off the car
/// (the two share the impact).
const VEHICLE_SHARE: f32 = 0.6;
/// Extra game time (s) the pre-crash slow motion waits for the impact.
const PRE_GRACE: f32 = 0.35;
/// Steered clear for this long (real s) during the run-up: cancel it.
const PRE_CLEAR: f32 = 0.4;

/// The slow-motion run-up to a predicted wreck.
struct PreCrash {
    start: std::time::Instant,
    cam: CrashCam,
    /// Since when the path ahead has been clear, if it is.
    clear_since: Option<std::time::Instant>,
}

#[derive(Default)]
pub struct Wreck {
    vehicle: i32,
    history: VecDeque<(i32, Vec3)>,
    last_impact: i32,
    /// Engine and body health a moment ago (restored after a prop hit).
    health: VecDeque<(i32, f32, f32)>,
    /// When the car was first seen jammed.
    stuck_since: Option<i32>,
    /// Slow motion already running ahead of a predicted wreck.
    pre: Option<PreCrash>,
    /// "DRIVEAWAY" is on screen since then (a predicted wreck that didn't
    /// happen).
    driveaway: Option<std::time::Instant>,
}

/// How long (real seconds) "DRIVEAWAY" stays on screen.
const DRIVEAWAY_TIME: f32 = 2.0;

pub enum Event {
    None,
    Impact(f32),
    Wrecked { dv: f32 },
    /// Wrecked by damage (fire, dead engine, jammed...), with the reason.
    Finished(&'static str),
    /// Knocked over a prop; speed and health given back.
    Prop(f32),
}

fn len_h(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn heading_of(x: f32, y: f32) -> f32 {
    (-x).atan2(y).to_degrees().rem_euclid(360.0)
}

impl Wreck {
    /// Forget the motion history (after a teleport, respawn or car change).
    pub fn reset(&mut self) {
        self.cancel_pre();
        self.history.clear();
        self.health.clear();
        self.stuck_since = None;
        self.vehicle = 0;
    }

    /// Ends a pre-crash slow motion that no wreck followed.
    fn cancel_pre(&mut self) {
        if let Some(mut pre) = self.pre.take() {
            pre.cam.finish();
            n::set_time_scale(1.0);
        }
    }

    /// Is the car all but certain to wreck within the lead time? Traces
    /// straight ahead along its path from the centre and 0.5 m either side;
    /// at least two must find a near head-on hit well over the threshold.
    fn wreck_ahead(s: &WreckSettings, veh: i32) -> bool {
        let v = n::get_entity_velocity(veh);
        let speed = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        if speed < s.wreck_dv * PRE_MARGIN || n::get_entity_speed_vector_relative(veh).y <= 0.0 {
            return false;
        }
        let dir = (v.x / speed, v.y / speed, v.z / speed);
        let hl = dir.0.hypot(dir.1).max(1e-3);
        let (rx, ry) = (dir.1 / hl, -dir.0 / hl);
        let p = n::get_entity_coords(veh);
        let reach = NOSE + speed * s.precrash_lead;
        let sure = [0.0, -CORNER, CORNER].into_iter().filter(|&side| {
            let from = Vec3::new(p.x + rx * side, p.y + ry * side, p.z + 0.2);
            let to = Vec3::new(from.x + dir.0 * reach, from.y + dir.1 * reach, from.z + dir.2 * reach);
            // Map and vehicles; props (objects) never wreck the car.
            let Some(hit) = n::raycast(from, to, 1 | 2, veh) else { return false };
            if hit.normal.z > 0.6 {
                return false; // the road ahead, a ramp or a landing
            }
            let head_on = -(dir.0 * hit.normal.x + dir.1 * hit.normal.y + dir.2 * hit.normal.z);
            if head_on < PRE_HEAD_ON {
                return false; // a glancing blow or a scrape
            }
            let into = if hit.entity != 0 && n::is_entity_a_vehicle(hit.entity) {
                let w = n::get_entity_velocity(hit.entity);
                ((v.x - w.x) * dir.0 + (v.y - w.y) * dir.1 + (v.z - w.z) * dir.2) * VEHICLE_SHARE
            } else {
                speed * head_on
            };
            into >= s.wreck_dv * PRE_MARGIN
        });
        sure.count() >= 2
    }

    /// Why the car is finished without a big crash, if it is.
    fn finished(&mut self, s: &WreckSettings, veh: i32, now: i32) -> Option<&'static str> {
        if !s.damage_wreck {
            return None;
        }
        if n::is_entity_on_fire(veh) {
            return Some("on fire");
        }
        if n::get_vehicle_engine_health(veh) < 0.0 {
            return Some("engine destroyed");
        }
        if !n::is_vehicle_driveable(veh) {
            return Some("undrivable");
        }
        // Wheels down, accelerator held, going nowhere. Upside down or on its
        // side doesn't count: GTA lets the driver roll the car back over.
        let jammed = n::get_control_normal(CONTROL_ACCELERATE) > 0.5 && n::get_entity_speed(veh) < 1.5 && n::is_vehicle_on_all_wheels(veh);
        if jammed {
            let since = *self.stuck_since.get_or_insert(now);
            if now - since >= (s.stuck_time * 1000.0) as i32 {
                return Some("jammed");
            }
        } else {
            self.stuck_since = None;
        }
        None
    }

    /// Watches the player's car; applies impact damage and runs a wreck,
    /// with the slow-motion crash camera starting just before a predicted
    /// one.
    pub fn update(&mut self, s: &WreckSettings, ped: i32, now: i32, props: &mut crate::propshield::PropShield) -> Event {
        let event = self.check(s, ped, now, props);
        if let Some(t) = self.driveaway {
            if t.elapsed().as_secs_f32() < DRIVEAWAY_TIME && !matches!(event, Event::Wrecked { .. } | Event::Finished(_)) {
                n::draw_text("DRIVEAWAY", 0.37, 0.40, 1.4, [255, 210, 0, 255]);
            } else {
                self.driveaway = None;
            }
        }
        match event {
            Event::None => {
                if self.vehicle == 0 {
                    self.cancel_pre();
                } else if self.pre.is_some() {
                    // Keep the run-up going until the impact; give up if it
                    // doesn't come, or the driver steers clear.
                    let ahead = Self::wreck_ahead(s, self.vehicle);
                    let pre = self.pre.as_mut().unwrap();
                    if ahead {
                        pre.clear_since = None;
                    } else if pre.clear_since.is_none() {
                        pre.clear_since = Some(std::time::Instant::now());
                    }
                    let t = pre.start.elapsed().as_secs_f32();
                    let steered_clear = pre.clear_since.is_some_and(|c| c.elapsed().as_secs_f32() > PRE_CLEAR);
                    if steered_clear || t > (s.precrash_lead + PRE_GRACE) / s.slowmo.max(0.05) {
                        self.cancel_pre();
                        self.driveaway = Some(std::time::Instant::now());
                    } else {
                        n::set_time_scale(s.slowmo);
                        pre.cam.update(self.vehicle, t);
                    }
                } else if s.precrash && Self::wreck_ahead(s, self.vehicle) {
                    let v = n::get_entity_velocity(self.vehicle);
                    n::set_time_scale(s.slowmo);
                    let cam = CrashCam::new(s.crash_cam, self.vehicle, heading_of(v.x, v.y));
                    self.pre = Some(PreCrash { start: std::time::Instant::now(), cam, clear_since: None });
                }
            }
            // Hit something after all, but not hard enough: back to normal.
            Event::Prop(_) | Event::Impact(_) => {
                if self.pre.is_some() {
                    self.cancel_pre();
                    self.driveaway = Some(std::time::Instant::now());
                }
            }
            _ => {}
        }
        event
    }

    fn check(&mut self, s: &WreckSettings, ped: i32, now: i32, props: &mut crate::propshield::PropShield) -> Event {
        if !s.enabled {
            return Event::None;
        }
        if !n::is_ped_in_any_vehicle(ped) {
            self.reset();
            return Event::None;
        }
        let veh = n::get_vehicle_ped_is_in(ped);
        // Cars, bikes and trucks, driven by the player.
        if matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) || n::get_ped_in_vehicle_seat(veh, -1) != ped {
            self.reset();
            return Event::None;
        }
        // Burnout drivers stay in the car.
        n::set_ped_config_flag(ped, 32, false);
        if veh != self.vehicle {
            self.reset();
            self.vehicle = veh;
        }
        // Damage wrecks only count while the prop shield isn't holding the
        // car's health (props must never wreck it).
        if props.shielding() {
            self.stuck_since = None;
        } else if let Some(why) = self.finished(s, veh, now) {
            let f = n::get_entity_forward_vector(veh);
            let v = n::get_entity_velocity(veh);
            let pre = self.pre.take();
            self.run_wreck(s, ped, veh, (0.0, 1.0), heading_of(f.x, f.y), len_h(v.x, v.y), pre);
            self.reset();
            return Event::Finished(why);
        }
        let v = n::get_entity_velocity(veh);
        self.history.push_back((now, v));
        while self.history.len() > 2 && now - self.history[1].0 >= WINDOW_MS {
            self.history.pop_front();
        }
        self.health.push_back((now, n::get_vehicle_engine_health(veh), n::get_vehicle_body_health(veh)));
        while self.health.len() > 2 && now - self.health[1].0 >= WINDOW_MS {
            self.health.pop_front();
        }
        let (t0, v0) = self.history[0];
        if now - t0 < 30 || now - self.last_impact < COOLDOWN_MS {
            return Event::None;
        }
        let (dx, dy) = (v.x - v0.x, v.y - v0.y);
        let dv = len_h(dx, dy);
        // Losing speed, not gaining it (e.g. being shunted from behind counts
        // only by how much the car's own motion changed).
        if dv < PROP_DV || len_h(v.x, v.y) > len_h(v0.x, v0.y) + 1.0 {
            return Event::None;
        }
        // Knocked over a prop: give back the speed and health it cost.
        let struck = (-dx / dv, -dy / dv);
        let hit = if crate::propshield::touching_vehicle(veh) { Vec::new() } else { props.struck_props(veh, struck) };
        if !hit.is_empty() {
            props.toss(veh, &hit[..1], v0, now);
            self.last_impact = now;
            let keep = s.prop_keep.clamp(0.0, 1.0);
            let (sx, sy) = (v0.x * keep, v0.y * keep);
            if len_h(sx, sy) > len_h(v.x, v.y) {
                n::set_entity_velocity(veh, sx, sy, v.z);
            }
            // Give back any health it cost.
            let (_, engine, body) = self.health[0];
            if n::get_vehicle_engine_health(veh) < engine {
                n::set_vehicle_engine_health(veh, engine);
            }
            if n::get_vehicle_body_health(veh) < body {
                n::set_vehicle_body_health(veh, body);
            }
            self.history.clear();
            return Event::Prop(dv);
        }
        if dv < s.impact_dv {
            return Event::None;
        }
        self.last_impact = now;

        // The struck side: opposite the change in velocity, in car space.
        let f = n::get_entity_forward_vector(veh);
        let (fx, fy) = (f.x, f.y);
        let (rx, ry) = (fy, -fx);
        let (hx, hy) = (-dx / dv, -dy / dv);
        let local = (hx * rx + hy * ry, hx * fx + hy * fy);
        let severity = ((dv - s.impact_dv) / (s.wreck_dv - s.impact_dv).max(1.0)).clamp(0.0, 1.0);
        dent(veh, local, 400.0 + 1600.0 * severity, 3 + (severity * 4.0) as i32);
        if severity > 0.5 {
            smash_side(veh, local);
        }
        if dv >= s.wreck_dv {
            let travel = heading_of(v0.x, v0.y);
            let pre = self.pre.take();
            self.run_wreck(s, ped, veh, local, travel, len_h(v0.x, v0.y), pre);
            self.reset();
            return Event::Wrecked { dv };
        }
        Event::Impact(dv)
    }

    fn run_wreck(&mut self, s: &WreckSettings, ped: i32, veh: i32, local: (f32, f32), travel: f32, speed: f32, pre: Option<PreCrash>) {
        let player = n::player_id();
        let model = n::get_entity_model(veh);
        // The prop shield may have the car protected; a wreck must show.
        n::set_vehicle_strong(veh, false);
        n::set_vehicle_can_be_visibly_damaged(veh, true);
        let look = n::vehicle_look(veh);
        n::set_player_invincible(player, true);
        n::set_entity_invincible(ped, true);
        n::request_model(model);

        // Smash it up: deep dents all around the struck side, glass, panels,
        // tyres, and a dead engine. The car is replaced afterwards, so
        // nothing here has to stay drivable.
        n::set_vehicle_can_deform_wheels(veh, true);
        n::set_vehicle_has_strong_axles(veh, false);
        n::set_vehicle_wheels_can_break_off(veh, true);
        for k in 0..4 {
            dent(veh, local, 3000.0, 6 + k);
        }
        dent(veh, (-local.0, -local.1), 1200.0, 3);
        crush(veh, local, s.crush);
        for w in 0..8 {
            n::smash_vehicle_window(veh, w);
        }
        for d in 0..6 {
            n::set_vehicle_door_broken(veh, d, false);
        }
        for t in [0, 1, 4, 5] {
            n::set_vehicle_tyre_burst(veh, t);
        }
        n::set_vehicle_engine_health(veh, -4000.0);
        n::set_vehicle_petrol_tank_health(veh, -1000.0);
        n::shake_gameplay_cam("LARGE_EXPLOSION_SHAKE", 0.6 + (speed / 40.0).min(1.0));
        n::set_time_scale(s.slowmo);
        let crash_pos = n::get_entity_coords(veh);
        // The vehicles and people in the crash, followed wherever they're
        // thrown so they can be removed with the wreck: everything close at
        // the impact, and anything the wreck touches as it tumbles.
        let mut involved = Involved::default();
        involved.near(ped, veh, crash_pos, INVOLVED_RADIUS);
        // The camera of the slow-motion run-up carries on, if there was one.
        let (mut cam, cam_start) = match pre {
            Some(p) => (p.cam, p.start),
            None => (CrashCam::new(s.crash_cam, veh, travel), std::time::Instant::now()),
        };

        // Real time: the game clock slows with the time scale.
        let start = std::time::Instant::now();
        let mut exploded = !s.explode;
        let mut crushes = 1;
        while start.elapsed().as_secs_f32() < s.crash_time {
            // Keep crumpling it as it tumbles (the game limits how far one
            // hit can push the body in, so it is done in rounds).
            if crushes < s.crush_rounds && start.elapsed().as_secs_f32() > crushes as f32 * 0.12 {
                crush(veh, local, s.crush);
                crushes += 1;
            }
            if !exploded && start.elapsed().as_secs_f32() > 0.4 {
                n::explode_vehicle(veh);
                n::add_explosion(n::get_entity_coords(veh), 7, 1.0, 1.0);
                exploded = true;
            }
            n::set_time_scale(s.slowmo);
            cam.update(veh, cam_start.elapsed().as_secs_f32());
            n::draw_text("WRECKED", 0.40, 0.40, 1.4, [255, 60, 30, 255]);
            n::set_entity_health(ped, n::get_entity_max_health(ped));
            involved.touching(ped, veh);
            shv::wait(0);
        }

        n::do_screen_fade_out(400);
        while !n::is_screen_faded_out() {
            n::set_time_scale(s.slowmo);
            cam.update(veh, cam_start.elapsed().as_secs_f32());
            n::draw_text("WRECKED", 0.40, 0.40, 1.4, [255, 60, 30, 255]);
            shv::wait(0);
        }
        n::set_time_scale(1.0);
        cam.finish();
        // Where it hit and where it ended up (it may have tumbled a way).
        let wreck_pos = n::get_entity_coords(veh);
        let cleared = if s.clear_radius > 0.0 { involved.remove() } else { 0 } + clear_around(ped, &[veh], &[crash_pos, wreck_pos], s.clear_radius);

        // Back on the nearest road, facing the way the player was going.
        let (spot, heading) = match n::closest_vehicle_node(crash_pos) {
            Some((p, h)) => {
                let d = (h - travel).rem_euclid(360.0);
                let h = if d > 90.0 && d < 270.0 { (h + 180.0).rem_euclid(360.0) } else { h };
                // Nodes sit on the road's centre line; keep right of it.
                let r = h.to_radians();
                (Vec3::new(p.x + r.cos() * 2.0, p.y + r.sin() * 2.0, p.z), h)
            }
            None => (crash_pos, travel),
        };
        n::clear_ped_tasks_immediately(ped);
        n::delete_vehicle(veh);
        n::set_entity_coords(ped, Vec3::new(spot.x, spot.y, spot.z + 1.0));
        let t0 = n::get_game_timer();
        while n::get_game_timer() - t0 < 5000 && !(n::has_model_loaded(model) && n::has_collision_loaded_around_entity(ped)) {
            n::request_collision_at_coord(spot);
            n::request_model(model);
            shv::wait(0);
        }
        let ground = n::raycast(Vec3::new(spot.x, spot.y, spot.z + 5.0), Vec3::new(spot.x, spot.y, spot.z - 20.0), 1, ped)
            .map(|h| h.pos.z)
            .unwrap_or(spot.z);
        let fresh = n::create_vehicle(model, Vec3::new(spot.x, spot.y, ground + 0.5), heading);
        if fresh != 0 {
            n::apply_vehicle_look(fresh, &look);
            n::set_entity_heading(fresh, heading);
            n::set_vehicle_on_ground_properly(fresh);
            n::set_ped_into_vehicle(ped, fresh);
            n::set_vehicle_engine_on(fresh);
            n::set_vehicle_as_no_longer_needed(fresh);
        }
        n::set_model_as_no_longer_needed(model);
        n::set_entity_health(ped, n::get_entity_max_health(ped));
        for _ in 0..10 {
            shv::wait(0);
        }
        // Again just before the screen comes back: traffic and people can be
        // spawned in while it is black, and the new car may be next to them.
        let again = clear_around(ped, &[fresh], &[crash_pos, wreck_pos, spot], s.clear_radius);
        crate::log(&format!("wreck clear-up: {} + {again} NPCs/vehicles removed", cleared));
        n::do_screen_fade_in(500);
        if fresh != 0 && s.respawn_speed > 0.0 {
            n::set_vehicle_forward_speed(fresh, s.respawn_speed);
        }
        // A moment of grace before the player can be hurt again.
        let t1 = n::get_game_timer();
        while n::get_game_timer() - t1 < 1500 {
            shv::wait(0);
        }
        n::set_entity_invincible(ped, false);
        n::set_player_invincible(player, false);
    }
}

/// Vehicles and people this close (m) to the car when it wrecks are part of
/// the crash.
const INVOLVED_RADIUS: f32 = 10.0;

/// The vehicles and NPCs caught up in a wreck.
#[derive(Default)]
struct Involved {
    vehicles: Vec<i32>,
    peds: Vec<i32>,
}

impl Involved {
    fn add_vehicle(&mut self, v: i32) {
        if !self.vehicles.contains(&v) {
            self.vehicles.push(v);
        }
    }

    fn add_ped(&mut self, p: i32) {
        if !self.peds.contains(&p) {
            self.peds.push(p);
        }
    }

    /// Everything within `radius` of `at`.
    fn near(&mut self, ped: i32, wreck: i32, at: Vec3, radius: f32) {
        let close = |e: i32| {
            let q = n::get_entity_coords(e);
            (q.x - at.x).powi(2) + (q.y - at.y).powi(2) + (q.z - at.z).powi(2) <= radius * radius
        };
        for v in shv::all_vehicles() {
            if v != wreck && close(v) {
                self.add_vehicle(v);
            }
        }
        for p in shv::all_peds() {
            if p != ped && !n::is_ped_a_player(p) && close(p) {
                self.add_ped(p);
            }
        }
    }

    /// Anything the wreck is touching now.
    fn touching(&mut self, ped: i32, wreck: i32) {
        let at = n::get_entity_coords(wreck);
        let close = |e: i32| {
            let q = n::get_entity_coords(e);
            (q.x - at.x).powi(2) + (q.y - at.y).powi(2) + (q.z - at.z).powi(2) <= 15.0 * 15.0
        };
        for v in shv::all_vehicles() {
            if v != wreck && !self.vehicles.contains(&v) && close(v) && n::is_entity_touching_entity(wreck, v) {
                self.add_vehicle(v);
            }
        }
        for p in shv::all_peds() {
            if p != ped && !self.peds.contains(&p) && !n::is_ped_a_player(p) && close(p) && n::is_entity_touching_entity(wreck, p) {
                self.add_ped(p);
            }
        }
    }

    /// Removes them all (and anyone in the vehicles), wherever they are now.
    fn remove(&self) -> usize {
        let mut removed = 0;
        for &v in &self.vehicles {
            if !n::does_entity_exist(v) {
                continue;
            }
            for seat in -1..8 {
                let p = n::get_ped_in_vehicle_seat(v, seat);
                if p != 0 && !n::is_ped_a_player(p) {
                    n::delete_ped(p);
                    removed += 1;
                }
            }
            n::delete_vehicle(v);
            removed += 1;
        }
        for &p in &self.peds {
            if n::does_entity_exist(p) && !n::is_ped_a_player(p) {
                n::delete_ped(p);
                removed += 1;
            }
        }
        removed
    }
}

/// Removes the NPCs and vehicles within `radius` of any of `spots` (except
/// the player and the vehicles in `keep`), and puts out fires there.
/// Returns how many were removed.
fn clear_around(ped: i32, keep: &[i32], spots: &[Vec3], radius: f32) -> usize {
    if radius <= 0.0 {
        return 0;
    }
    let near = |e: i32| {
        let q = n::get_entity_coords(e);
        spots.iter().any(|at| (q.x - at.x).powi(2) + (q.y - at.y).powi(2) + (q.z - at.z).powi(2) <= radius * radius)
    };
    let vehicles: Vec<i32> = shv::all_vehicles().into_iter().filter(|&v| !keep.contains(&v) && near(v)).collect();
    let mut removed = 0;
    // Peds first (including anyone in those vehicles), then the vehicles.
    for p in shv::all_peds() {
        if p == ped || n::is_ped_a_player(p) || !n::does_entity_exist(p) {
            continue;
        }
        let in_cleared = n::is_ped_in_any_vehicle(p) && vehicles.contains(&n::get_vehicle_ped_is_in(p));
        if in_cleared || near(p) {
            n::delete_ped(p);
            removed += 1;
        }
    }
    for v in vehicles {
        if n::does_entity_exist(v) {
            n::delete_vehicle(v);
            removed += 1;
        }
    }
    // The game's own sweep catches anything the lists missed (it leaves the
    // player and the car they're in alone), and the explosion's fires go.
    for &at in spots {
        n::clear_area_of_peds(at, radius);
        n::clear_area_of_vehicles(at, radius);
        n::stop_fire_in_range(at, radius);
    }
    removed
}

/// Burnout-style crash camera: low and off to the side of the car's path,
/// slowly circling the wreck as it tumbles, kept out of walls.
struct CrashCam {
    cam: i32,
    /// Starting angle around the car (radians, world space).
    angle: f32,
}

impl CrashCam {
    const RADIUS: f32 = 7.5;
    const HEIGHT: f32 = 1.6;
    /// Orbit speed, radians per real second.
    const SPIN: f32 = 0.35;

    fn new(enabled: bool, veh: i32, travel: f32) -> Self {
        if !enabled {
            return Self { cam: 0, angle: 0.0 };
        }
        let cam = n::create_cam();
        n::set_cam_fov(cam, 50.0);
        n::point_cam_at_entity(cam, veh);
        // Ahead and to the right of where the car was heading, looking back
        // at it as it comes in.
        let h = travel.to_radians();
        let (fx, fy) = (-h.sin(), h.cos());
        let angle = (fy * 0.6 - fx).atan2(fx * 0.6 + fy) ;
        let mut c = Self { cam, angle };
        c.update(veh, 0.0);
        n::set_cam_active(cam, true);
        n::render_script_cams(true, 0);
        c
    }

    fn update(&mut self, veh: i32, t: f32) {
        if self.cam == 0 {
            return;
        }
        let p = n::get_entity_coords(veh);
        let a = self.angle + t * Self::SPIN;
        let want = Vec3::new(p.x + a.cos() * Self::RADIUS, p.y + a.sin() * Self::RADIUS, p.z + Self::HEIGHT);
        // Don't put the camera inside a wall or the ground.
        let from = Vec3::new(p.x, p.y, p.z + 1.0);
        let at = match n::raycast(from, want, 1, veh) {
            Some(h) => {
                let (dx, dy, dz) = (h.pos.x - from.x, h.pos.y - from.y, h.pos.z - from.z);
                let l = (dx * dx + dy * dy + dz * dz).sqrt().max(0.01);
                let k = ((l - 0.6) / l).max(0.0);
                Vec3::new(from.x + dx * k, from.y + dy * k, from.z + dz * k)
            }
            None => want,
        };
        n::set_cam_coord(self.cam, at);
    }

    fn finish(&mut self) {
        if self.cam != 0 {
            n::render_script_cams(false, 0);
            n::set_cam_active(self.cam, false);
            n::destroy_cam(self.cam);
            self.cam = 0;
        }
    }
}

/// Crumples the whole car: hits from every side and from above, heaviest on
/// the struck side. With `focusOnModel` each hit travels in from outside the
/// car until it meets the body.
fn crush(veh: i32, local: (f32, f32), damage: f32) {
    // Struck face, hardest.
    for k in 0..6 {
        let spread = (k as f32 / 5.0 - 0.5) * 2.0;
        let (x, y) = local;
        n::set_vehicle_damage(veh, x * 2.5 - y * spread, y * 3.5 + x * spread, 0.3, damage * 1.5, 2.0);
    }
    // All round: front, back, both sides, corners.
    for (x, y) in [(0.0, 3.5), (0.0, -3.5), (2.2, 0.0), (-2.2, 0.0), (1.8, 2.5), (-1.8, 2.5), (1.8, -2.5), (-1.8, -2.5)] {
        n::set_vehicle_damage(veh, x, y, 0.2, damage, 1.8);
    }
    // Roof caved in.
    for y in [-1.0, 0.0, 1.0] {
        n::set_vehicle_damage(veh, 0.0, y, 2.5, damage, 1.8);
    }
}

/// Dents a car around the struck side (`local`: unit direction in car space,
/// x right, y forward), spread over `hits` points.
fn dent(veh: i32, local: (f32, f32), damage: f32, hits: i32) {
    let (x, y) = local;
    for k in 0..hits {
        // Fan the hits across the struck face.
        let spread = (k as f32 / (hits.max(2) - 1) as f32 - 0.5) * 1.6;
        let (px, py) = (x * 1.0 + -y * spread * 0.6, y * 2.2 + x * spread);
        n::set_vehicle_damage(veh, px, py, 0.1 + (k % 2) as f32 * 0.3, damage, 1.2);
    }
}

/// Breaks glass and the panel nearest the struck side.
fn smash_side(veh: i32, local: (f32, f32)) {
    let (x, y) = local;
    if y > 0.6 {
        n::smash_vehicle_window(veh, 6); // windscreen
        n::set_vehicle_door_broken(veh, 4, false); // bonnet
    } else if y < -0.6 {
        n::smash_vehicle_window(veh, 7);
        n::set_vehicle_door_broken(veh, 5, false); // boot
    } else if x > 0.0 {
        n::smash_vehicle_window(veh, 1);
        n::smash_vehicle_window(veh, 3);
        n::set_vehicle_door_broken(veh, 1, false);
    } else {
        n::smash_vehicle_window(veh, 0);
        n::smash_vehicle_window(veh, 2);
        n::set_vehicle_door_broken(veh, 0, false);
    }
}
