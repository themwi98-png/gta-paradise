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
//! fire, engine dead, no longer drivable, or not moving with the accelerator
//! held (wheels jammed) for `@wreck_stuck` seconds.
//!
//! As in Burnout, damage that changes how the car drives is a wreck too;
//! only looks may be smashed up. A wheel knocked out of line (axle bent,
//! wheel pushed in or torn off, so the car pulls or won't steer) by more than
//! `@wreck_wheel` cm, or an engine damaged below `@wreck_engine` health
//! (smoking, down on power). To make that rarer the player's axles are
//! strong (`@strong_axles`): knocks don't bend them, only a crash that
//! crushes the body in around a wheel moves it. The dents added after a hit
//! (below) never move a wheel; only the crash itself does. The player's
//! tyres can't burst at all (`@strong_tyres`).
//!
//! Ramming a police car (a shunt, slam, T-bone or landing on it; see
//! takedown.rs) is never a wreck either: as in Burnout the attacker is
//! favoured, so the car keeps `@takedown_keep` of its speed and all its
//! health. A wreck soon after a police car rammed the player is TAKEN DOWN
//! instead of WRECKED. With a police car near, the driver is never thrown
//! through the windscreen.
//!
//! Knocking over a prop (lamp post, bin, sign...) is not a crash: the car
//! keeps `@prop_keep` of its speed and the hit costs it no health (the prop
//! shield, propshield.rs, already stops it denting the car).
//!
//! Once the screen has faded after a wreck, NPCs and vehicles within
//! CLEAR_RADIUS of the wreck are removed, so the respawn starts clean; a
//! police car that took the player down (and its crew) stays.
//!
//! The player is kept invincible through a wreck, so GTA's own death and
//! hospital respawn never happen for one; the wreck is the death. With
//! `@wreck_eject` GTA's own windscreen ejection is switched on, but only
//! during the slow-motion run-up to a wreck that is all but certain (see
//! above), so normal driving and prop hits never throw the driver out. A
//! crash that does is always a wreck, and the crash camera frames both car
//! and driver, pulling back as they fly apart.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::VecDeque;

/// `@...` wreck settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct WreckSettings {
    pub enabled: bool,
    /// Sudden change in speed (m/s) for a wreck; 25 m/s is about 90 km/h.
    pub wreck_dv: f32,
    /// Sudden change in speed (m/s) for extra crash damage.
    pub impact_dv: f32,
    /// Blow the car up on a wreck.
    pub explode: bool,
    /// Throw the driver through the windscreen in a wreck.
    pub eject: bool,
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
    /// After a prop hit, the car can't be spun: for PROP_HOLD_MS it keeps
    /// the turn rate it had (straight stays straight, a drift keeps
    /// turning as it was).
    pub prop_straight: bool,
    /// Start the slow-motion crash camera just before a predicted wreck.
    pub precrash: bool,
    /// How far ahead (game seconds) a wreck is predicted.
    pub precrash_lead: f32,
    /// A hit at least this hard (sudden speed change, m/s) shows the
    /// slow-motion crash camera from the moment of impact; one that isn't a
    /// wreck ends with DRIVEAWAY.
    pub crash_cam_dv: f32,
    /// How long that camera stays (real seconds) when it isn't a wreck.
    pub crash_cam_hold: f32,
    /// How much colour drains out of the picture on a wreck (0 = none,
    /// 1 = black and white).
    pub desat: f32,
    /// NPCs and vehicles this close (m) to the wreck are removed before the
    /// respawn (0: off).
    pub clear_radius: f32,
    /// A wheel knocked this far out of line (m, across or along the car)
    /// wrecks it (0: off).
    pub wheel_shift: f32,
    /// The player's tyres can't burst.
    pub strong_tyres: bool,
    /// Engine health under this wrecks the car (GTA: 1000 new, smoking and
    /// losing power under about 400).
    pub engine_wreck: f32,
    /// The player's car has strong axles (knocks don't bend them).
    pub strong_axles: bool,
}

impl Default for WreckSettings {
    fn default() -> Self {
        Self { enabled: true, wreck_dv: 25.0, impact_dv: 11.0, explode: true, eject: true, respawn_speed: 12.0, crash_time: 5.0, slowmo: 0.15, crash_cam: true, crush: 8000.0, crush_rounds: 8, damage_wreck: true, stuck_time: 2.5, prop_keep: 1.0, prop_straight: true, precrash: true, precrash_lead: 0.12, crash_cam_dv: 20.0, crash_cam_hold: 1.5, clear_radius: 6.0, desat: 0.6, wheel_shift: 0.0, strong_tyres: true, engine_wreck: 100.0, strong_axles: true }
    }
}

/// Wheel bones.
const WHEELS: [&str; 6] = ["wheel_lf", "wheel_rf", "wheel_lm1", "wheel_rm1", "wheel_lr", "wheel_rr"];

/// INPUT_VEH_MOVE_LR (steering).
const CONTROL_STEER: i32 = 59;
/// Steering and suspension travel move a wheel's hub several cm on their
/// own, so wheels are only measured while the car is steady: on all four
/// wheels with the steering centred for this long (s)...
const WHEEL_STEADY: f32 = 0.4;
/// ...and a wheel must stay out of line for this much steady time (s).
const WHEEL_PERSIST: f32 = 1.0;

/// Where a car's wheels sat when the player got it, to tell when one has
/// been knocked out of line.
#[derive(Default)]
struct Wheels {
    vehicle: i32,
    /// Wheel name, bone and resting place in car space (x right, y forward);
    /// taken the first time the car is steady.
    rest: Vec<(&'static str, i32, f32, f32)>,
    measured: bool,
    /// Since when the car has been steady.
    steady_since: Option<std::time::Instant>,
    /// Steady time (s) a wheel has been out of line without a break.
    out_for: f32,
    /// The worst shift logged so far (m), so the log shows it growing.
    logged: f32,
}

fn hub(veh: i32, bone: i32) -> Vec3 {
    n::get_offset_from_entity_given_world_coords(veh, n::get_world_position_of_entity_bone(veh, bone))
}

impl Wheels {
    /// Starts watching a car.
    fn watch(&mut self, veh: i32) {
        if self.vehicle != veh {
            *self = Self { vehicle: veh, ..Default::default() };
        }
    }

    /// The wheel furthest out of line now and how far (m, across or along
    /// the car; up and down is the suspension), if the car is steady enough
    /// to tell.
    fn shift(&mut self, veh: i32) -> Option<(&'static str, f32)> {
        if !n::is_vehicle_on_all_wheels(veh) || n::get_control_normal(CONTROL_STEER).abs() > 0.1 {
            self.steady_since = None;
            return None;
        }
        if self.steady_since.get_or_insert_with(std::time::Instant::now).elapsed().as_secs_f32() < WHEEL_STEADY {
            return None;
        }
        if !self.measured {
            self.measured = true;
            self.rest = WHEELS
                .iter()
                .filter_map(|&name| {
                    let bone = n::get_entity_bone_index_by_name(veh, name);
                    (bone >= 0).then(|| {
                        let p = hub(veh, bone);
                        (name, bone, p.x, p.y)
                    })
                })
                .collect();
            return None;
        }
        let mut worst = ("", 0.0);
        for &(name, bone, x, y) in &self.rest {
            let p = hub(veh, bone);
            let d = len_h(p.x - x, p.y - y);
            if d > worst.1 {
                worst = (name, d);
            }
        }
        Some(worst)
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
/// How fast steering turns a car held straight while ramming (degrees a
/// second at full lock).
const STRAIGHT_STEER: f32 = 70.0;
/// A car keeps its turn this long (ms) after its last prop hit.
const PROP_HOLD_MS: i32 = 200;
/// How hard a car held straight is turned back to its heading (yaw rate
/// per radian off, 1/s), and the fastest it's turned (rad/s).
const STRAIGHT_PULL: f32 = 12.0;
const STRAIGHT_MAX_YAW: f32 = 4.0;

/// The slow-motion run-up to a predicted wreck.
struct PreCrash {
    start: std::time::Instant,
    cam: CrashCam,
    /// Started at an impact (the crash camera for any hard hit) rather than
    /// ahead of a predicted wreck.
    impact: bool,
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
    /// The big-message movie for the WRECKED screen (0: not requested yet).
    big_message: i32,
    /// Where the car's wheels should be.
    wheels: Wheels,
    /// When the car was last airborne (game ms).
    last_air: i32,
    /// The car's heading and turn rate (yaw, rad/s) over the last
    /// WINDOW_MS, like `history`.
    headings: VecDeque<(i32, f32, f32)>,
    /// Held straight while ramming a police car: (heading, travel
    /// direction x, y) from just before the hit.
    straight: Option<(f32, f32, f32)>,
    /// After a prop hit: the turn rate it had just before, kept.
    prop_spin: Option<f32>,
}

/// A vehicle moving towards the car faster than this (m/s) is oncoming.
const ONCOMING: f32 = 5.0;

/// How long (real seconds) "DRIVEAWAY" stays on screen.
const DRIVEAWAY_TIME: f32 = 2.0;

pub enum Event {
    None,
    Impact(f32),
    Wrecked { dv: f32 },
    /// Wrecked by damage (fire, dead engine, wheel out of line, jammed...),
    /// with the reason.
    Finished(String),
    /// Knocked over a prop; speed and health given back.
    Prop(f32),
    /// Rammed a police car (takedown.rs); speed and health given back.
    Attack(f32),
    /// A hit while grinding (grind.rs): never a crash.
    Grind(f32),
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
        self.headings.clear();
        self.straight = None;
        self.prop_spin = None;
        self.health.clear();
        self.stuck_since = None;
        self.vehicle = 0;
    }

    /// While ramming: the car keeps the heading and direction of travel it
    /// had just before the hit (the oldest in the history, before the
    /// contact turned it): any spin is countered; the player's steering
    /// still turns both. Its speed is left alone (the ram's slowing down
    /// stays).
    fn hold_straight(&mut self, veh: i32, why: &str) {
        let v = n::get_entity_velocity(veh);
        let (mut heading, mut dx, mut dy) = *self.straight.get_or_insert_with(|| {
            let heading = self.headings.front().map_or_else(|| n::get_entity_heading(veh), |h| h.1);
            let v0 = self.history.front().map_or(v, |h| h.1);
            let len = len_h(v0.x, v0.y);
            let (dx, dy) = if len > 1.0 { (v0.x / len, v0.y / len) } else { (-heading.to_radians().sin(), heading.to_radians().cos()) };
            crate::log(&format!("holding the car straight {why} (heading {heading:.0})"));
            (heading, dx, dy)
        });
        // Steering (left -1 .. right 1) turns the held heading; GTA's
        // heading goes anticlockwise.
        let turn = -n::get_control_normal(CONTROL_STEER) * STRAIGHT_STEER * n::get_frame_time();
        if turn != 0.0 {
            heading = (heading + turn).rem_euclid(360.0);
            let (s, c) = turn.to_radians().sin_cos();
            (dx, dy) = (dx * c - dy * s, dx * s + dy * c);
            self.straight = Some((heading, dx, dy));
        }
        // Turned back towards the held heading by its yaw rate (rad/s,
        // anticlockwise like the heading), not by setting the heading: that
        // also levelled the car and reset its suspension every frame, which
        // sank it onto its rims.
        let off = (heading - n::get_entity_heading(veh) + 540.0).rem_euclid(360.0) - 180.0;
        let w = n::get_entity_rotation_velocity(veh);
        n::set_entity_angular_velocity(veh, w.x, w.y, (off.to_radians() * STRAIGHT_PULL).clamp(-STRAIGHT_MAX_YAW, STRAIGHT_MAX_YAW));
        let speed = len_h(v.x, v.y);
        n::set_entity_velocity(veh, dx * speed, dy * speed, v.z);
    }

    /// Just after a prop hit: the car keeps the turn rate (yaw) it had the
    /// frame before the hit, so the prop can't spin it, whether it was
    /// going straight or drifting. Nothing else about its motion is
    /// touched (holding a heading and a direction of travel, as when
    /// ramming, set drifting cars straight and jerked them about).
    fn keep_turn(&mut self, veh: i32, hit_at: i32) {
        let w = n::get_entity_rotation_velocity(veh);
        let yaw = *self.prop_spin.get_or_insert_with(|| {
            let yaw = self.headings.iter().rev().find(|h| h.0 < hit_at).map_or(w.z, |h| h.2);
            crate::log(&format!("prop hit: keeping the car's turn ({:.0} deg/s)", yaw.to_degrees()));
            yaw
        });
        n::set_entity_angular_velocity(veh, w.x, w.y, yaw);
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
    fn wreck_ahead(s: &WreckSettings, veh: i32, rams: (bool, bool)) -> bool {
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
                // A police car (or with `@takedown_any` any vehicle) not
                // coming at us (going our way, crossing, parked) gets rammed
                // (takedown.rs).
                let coming = -(w.x * dir.0 + w.y * dir.1 + w.z * dir.2);
                if rams.0 && coming < ONCOMING && (rams.1 || crate::takedown::is_police(hit.entity)) {
                    return false;
                }
                ((v.x - w.x) * dir.0 + (v.y - w.y) * dir.1 + (v.z - w.z) * dir.2) * VEHICLE_SHARE
            } else {
                speed * head_on
            };
            into >= s.wreck_dv * PRE_MARGIN
        });
        sure.count() >= 2
    }

    /// Why the car is finished without a big crash, if it is.
    fn finished(&mut self, s: &WreckSettings, veh: i32, now: i32) -> Option<String> {
        if !s.damage_wreck {
            return None;
        }
        if n::is_entity_on_fire(veh) {
            return Some("on fire".into());
        }
        let engine = n::get_vehicle_engine_health(veh);
        if engine < 0.0 {
            return Some("engine destroyed".into());
        }
        if !n::is_vehicle_driveable(veh) {
            return Some("undrivable".into());
        }
        // Damage that changes how it drives (Burnout: only looks may be
        // smashed up).
        if engine < s.engine_wreck {
            return Some(format!("engine damaged, health {engine:.0}"));
        }
        if let Some((w, shift)) = self.wheels.shift(veh) {
            if s.wheel_shift > 0.0 && shift > s.wheel_shift {
                self.wheels.out_for += n::get_frame_time();
                if self.wheels.out_for >= WHEEL_PERSIST {
                    return Some(format!("{w} {:.0} cm out of line", shift * 100.0));
                }
            } else {
                self.wheels.out_for = 0.0;
            }
            // For tuning @wreck_wheel: how far wheels get knocked without a
            // wreck.
            if shift >= self.wheels.logged + 0.01 {
                self.wheels.logged = shift;
                crate::log(&format!("{w} {:.1} cm out of line (wrecks over {:.0})", shift * 100.0, s.wheel_shift * 100.0));
            }
        }
        // Wheels down, accelerator held, going nowhere. Upside down or on its
        // side doesn't count: GTA lets the driver roll the car back over.
        let jammed = n::get_control_normal(CONTROL_ACCELERATE) > 0.5 && n::get_entity_speed(veh) < 1.5 && n::is_vehicle_on_all_wheels(veh);
        if jammed {
            let since = *self.stuck_since.get_or_insert(now);
            if now - since >= (s.stuck_time * 1000.0) as i32 {
                return Some("jammed".into());
            }
        } else {
            self.stuck_since = None;
        }
        None
    }

    /// Watches the player's car; applies impact damage and runs a wreck,
    /// with the slow-motion crash camera starting just before a predicted
    /// one.
    pub fn update(&mut self, s: &WreckSettings, ped: i32, now: i32, props: &mut crate::propshield::PropShield, ctx: &crate::takedown::Context) -> Event {
        if self.big_message == 0 {
            self.big_message = n::request_scaleform_movie(BIG_MESSAGE);
        }
        let event = self.check(s, ped, now, props, ctx);
        let rams = (ctx.rams_police, ctx.rams_any);
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
                } else if self.pre.as_ref().is_some_and(|p| p.impact) {
                    // The crash camera after a hard hit: held for a moment,
                    // then back to driving (it wasn't a wreck).
                    let pre = self.pre.as_mut().unwrap();
                    let t = pre.start.elapsed().as_secs_f32();
                    if t > s.crash_cam_hold {
                        self.cancel_pre();
                        self.driveaway = Some(std::time::Instant::now());
                    } else {
                        n::set_time_scale(s.slowmo);
                        pre.cam.update(self.vehicle, t);
                    }
                } else if self.pre.is_some() {
                    // Keep the run-up going until the impact; give up if it
                    // doesn't come, or the driver steers clear.
                    let ahead = Self::wreck_ahead(s, self.vehicle, rams);
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
                } else if s.precrash && Self::wreck_ahead(s, self.vehicle, rams) {
                    let v = n::get_entity_velocity(self.vehicle);
                    n::set_time_scale(s.slowmo);
                    let cam = CrashCam::new(s.crash_cam, self.vehicle, heading_of(v.x, v.y));
                    self.pre = Some(PreCrash { start: std::time::Instant::now(), cam, impact: false, clear_since: None });
                }
            }
            // Ramming a police car or grinding: not a crash, and nothing
            // to show.
            Event::Attack(_) | Event::Grind(_) => {
                if self.pre.as_ref().is_some_and(|p| !p.impact) {
                    self.cancel_pre();
                }
            }
            // A prop: never a crash. Ends a wreck run-up that didn't come.
            Event::Prop(_) => {
                if self.pre.as_ref().is_some_and(|p| !p.impact) {
                    self.cancel_pre();
                    self.driveaway = Some(std::time::Instant::now());
                }
            }
            // A hit, but not a wreck: the crash camera from the impact (it
            // carries on from a wreck run-up's camera), then DRIVEAWAY.
            Event::Impact(dv) => {
                let hard = s.crash_cam && dv >= s.crash_cam_dv;
                match self.pre.as_mut() {
                    Some(p) if hard => {
                        p.impact = true;
                        p.start = std::time::Instant::now();
                    }
                    Some(p) if !p.impact => {
                        self.cancel_pre();
                        self.driveaway = Some(std::time::Instant::now());
                    }
                    Some(_) => {}
                    None if hard => {
                        let v = n::get_entity_velocity(self.vehicle);
                        n::set_time_scale(s.slowmo);
                        let cam = CrashCam::new(s.crash_cam, self.vehicle, heading_of(v.x, v.y));
                        self.pre = Some(PreCrash { start: std::time::Instant::now(), cam, impact: true, clear_since: None });
                    }
                    None => {}
                }
            }
            _ => {}
        }
        event
    }

    fn check(&mut self, s: &WreckSettings, ped: i32, now: i32, props: &mut crate::propshield::PropShield, ctx: &crate::takedown::Context) -> Event {
        if !s.enabled {
            return Event::None;
        }
        // Taken down by a police car (which the clear-up spares), or a
        // wreck of the player's own.
        let word = ctx.taken_down.unwrap_or("WRECKED");
        let spare = ctx.taken_by;
        if !n::is_ped_in_any_vehicle(ped) {
            // Thrown out through the windscreen (GTA's own, `@wreck_eject`):
            // always a wreck.
            if let Some(event) = self.thrown_out(s, ped, word, spare) {
                self.reset();
                return event;
            }
            self.reset();
            return Event::None;
        }
        let veh = n::get_vehicle_ped_is_in(ped);
        // Cars, bikes and trucks, driven by the player.
        if matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) || n::get_ped_in_vehicle_seat(veh, -1) != ped {
            self.reset();
            return Event::None;
        }
        // Burnout drivers stay in the car, except that GTA's windscreen
        // ejection (`@wreck_eject`) is on in the run-up to a certain wreck;
        // never near police (takedowns, police ramming).
        let certain_wreck = self.pre.as_ref().is_some_and(|p| !p.impact) && !ctx.police_near;
        n::set_ped_config_flag(ped, 32, s.eject && certain_wreck);
        if veh != self.vehicle {
            self.reset();
            self.vehicle = veh;
        }
        self.wheels.watch(veh);
        n::set_vehicle_has_strong_axles(veh, s.strong_axles);
        n::set_vehicle_tyres_can_burst(veh, !s.strong_tyres);
        // Ramming a police car costs no health (takedown.rs), and with
        // `@takedown_straight` can't spin the car either.
        if ctx.attacking {
            if let Some(&(_, engine, body)) = self.health.back() {
                give_back_health(veh, engine, body);
            }
        }
        let prop_hit = props.last_hit() != 0 && now - props.last_hit() < PROP_HOLD_MS;
        if ctx.attacking && ctx.straight {
            self.prop_spin = None;
            self.hold_straight(veh, "ramming");
        } else {
            if self.straight.take().is_some() {
                crate::log("car let go (it was held straight)");
            }
            if prop_hit && s.prop_straight {
                self.keep_turn(veh, props.last_hit());
            } else if self.prop_spin.take().is_some() {
                crate::log("prop hit: car's turn let go");
            }
        }
        // Damage wrecks only count while the prop shield isn't holding the
        // car's health (props must never wreck it).
        if props.shielding() {
            self.stuck_since = None;
        } else if let Some(why) = self.finished(s, veh, now) {
            let f = n::get_entity_forward_vector(veh);
            let v = n::get_entity_velocity(veh);
            let pre = self.pre.take();
            self.run_wreck(s, ped, veh, (word, spare), (0.0, 1.0), heading_of(f.x, f.y), len_h(v.x, v.y), pre);
            self.reset();
            return Event::Finished(why);
        }
        let v = n::get_entity_velocity(veh);
        if n::is_entity_in_air(veh) {
            self.last_air = now;
        }
        self.history.push_back((now, v));
        while self.history.len() > 2 && now - self.history[1].0 >= WINDOW_MS {
            self.history.pop_front();
        }
        self.headings.push_back((now, n::get_entity_heading(veh), n::get_entity_rotation_velocity(veh).z));
        while self.headings.len() > 2 && now - self.headings[1].0 >= WINDOW_MS {
            self.headings.pop_front();
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
            give_back_health(veh, engine, body);
            self.history.clear();
            return Event::Prop(dv);
        }
        // Ramming a police car (takedown.rs): as in Burnout the attacker is
        // favoured. Never a wreck; most of the speed it cost comes back.
        if ctx.attacking {
            self.last_impact = now;
            let (kx, ky) = (v0.x * ctx.keep, v0.y * ctx.keep);
            if len_h(kx, ky) > len_h(v.x, v.y) {
                n::set_entity_velocity(veh, kx, ky, v.z);
            }
            self.history.clear();
            return Event::Attack(dv);
        }
        // Grinding along a wall or a car: never a crash (grind.rs keeps the
        // speed and most of the health).
        if ctx.grinding {
            if dv >= s.impact_dv {
                self.last_impact = now;
                self.history.clear();
                return Event::Grind(dv);
            }
            return Event::None;
        }
        if dv < s.impact_dv.min(if s.crash_cam { s.crash_cam_dv } else { f32::MAX }) {
            return Event::None;
        }
        self.last_impact = now;

        // The struck side: opposite the change in velocity, in car space.
        let f = n::get_entity_forward_vector(veh);
        let (fx, fy) = (f.x, f.y);
        let (rx, ry) = (fy, -fx);
        let (hx, hy) = (-dx / dv, -dy / dv);
        let local = (hx * rx + hy * ry, hx * fx + hy * fy);
        if dv >= s.impact_dv {
            let severity = ((dv - s.impact_dv) / (s.wreck_dv - s.impact_dv).max(1.0)).clamp(0.0, 1.0);
            dent(veh, local, 400.0 + 1600.0 * severity, 3 + (severity * 4.0) as i32);
            if severity > 0.5 {
                smash_side(veh, local);
            }
        }
        if dv >= s.wreck_dv {
            // What it was, for the log.
            let what = if crate::propshield::touching_vehicle(veh) { "against a vehicle" } else { "not touching a vehicle" };
            let air = if now - self.last_air < 500 { format!(", landing ({} ms after being airborne)", now - self.last_air) } else { String::new() };
            crate::log(&format!(
                "wreck hit: {:.0} -> {:.0} km/h in {} ms, height speed {:.1} -> {:.1} m/s, {what}{air}, struck side x {:.2} y {:.2}",
                len_h(v0.x, v0.y) * 3.6,
                len_h(v.x, v.y) * 3.6,
                now - t0,
                v0.z,
                v.z,
                local.0,
                local.1
            ));
            let travel = heading_of(v0.x, v0.y);
            let pre = self.pre.take();
            self.run_wreck(s, ped, veh, (word, spare), local, travel, len_h(v0.x, v0.y), pre);
            self.reset();
            return Event::Wrecked { dv };
        }
        Event::Impact(dv)
    }

    /// The driver was just thrown out of the car by a crash (out of it,
    /// ragdolling, the car fast a moment ago and its speed changed sharply):
    /// runs the wreck.
    fn thrown_out(&mut self, s: &WreckSettings, ped: i32, word: &'static str, spare: i32) -> Option<Event> {
        let veh = self.vehicle;
        if !s.eject || veh == 0 || !n::does_entity_exist(veh) || !n::is_ped_ragdoll(ped) {
            return None;
        }
        let &(_, v0) = self.history.front()?;
        let v = n::get_entity_velocity(veh);
        let (dx, dy) = (v.x - v0.x, v.y - v0.y);
        let dv = len_h(dx, dy);
        if len_h(v0.x, v0.y) < EJECT_MIN_SPEED || dv < s.impact_dv {
            return None;
        }
        // The struck side, in car space, as for any impact.
        let f = n::get_entity_forward_vector(veh);
        let (hx, hy) = (-dx / dv, -dy / dv);
        let local = (hx * f.y - hy * f.x, hx * f.x + hy * f.y);
        crate::log(&format!("wreck: driver thrown through the windscreen at {:.0} km/h", len_h(v0.x, v0.y) * 3.6));
        let pre = self.pre.take();
        self.run_wreck(s, ped, veh, (word, spare), local, heading_of(v0.x, v0.y), len_h(v0.x, v0.y), pre);
        Some(Event::Wrecked { dv })
    }

    /// `blame`: WRECKED, or TAKEN DOWN by a police car, and that car (spared
    /// by the clear-up; 0: none).
    #[allow(clippy::too_many_arguments)]
    fn run_wreck(&mut self, s: &WreckSettings, ped: i32, veh: i32, blame: (&'static str, i32), local: (f32, f32), travel: f32, speed: f32, pre: Option<PreCrash>) {
        let (word, spare) = blame;
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
        n::set_vehicle_tyres_can_burst(veh, true);
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
        let mut involved = Involved { spare, ..Default::default() };
        involved.near(ped, veh, crash_pos, INVOLVED_RADIUS);
        // The camera of the slow-motion run-up carries on, if there was one.
        let mut banner = Banner::new(word, self.big_message, s.desat);
        let (mut cam, cam_start) = match pre {
            Some(p) => (p.cam, p.start),
            None => (CrashCam::new(s.crash_cam, veh, travel), std::time::Instant::now()),
        };

        // Real time: the game clock slows with the time scale.
        let start = std::time::Instant::now();
        let mut exploded = !s.explode;
        let mut crushes = 1;
        while start.elapsed().as_secs_f32() < s.crash_time {
            // Thrown out through the windscreen (at the impact, or in the
            // first moments of the crash): frame the driver too.
            if s.eject && cam.follow == 0 && !n::is_ped_in_any_vehicle(ped) {
                cam.follow = ped;
            }
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
            banner.draw(cam.cam);
            n::set_entity_health(ped, n::get_entity_max_health(ped));
            involved.touching(ped, veh);
            shv::wait(0);
        }

        n::do_screen_fade_out(400);
        while !n::is_screen_faded_out() {
            n::set_time_scale(s.slowmo);
            cam.update(veh, cam_start.elapsed().as_secs_f32());
            banner.draw(cam.cam);
            shv::wait(0);
        }
        n::set_time_scale(1.0);
        cam.finish();
        banner.finish();
        // Where it hit and where it ended up (it may have tumbled a way).
        let wreck_pos = n::get_entity_coords(veh);
        let cleared = if s.clear_radius > 0.0 { involved.remove() } else { 0 } + clear_around(ped, &[veh], spare, &[crash_pos, wreck_pos], s.clear_radius);

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
        // Up from a ragdoll before going into the new car.
        for _ in 0..10 {
            if !n::is_ped_ragdoll(ped) {
                break;
            }
            shv::wait(0);
            n::clear_ped_tasks_immediately(ped);
        }
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
        let again = clear_around(ped, &[fresh], spare, &[crash_pos, wreck_pos, spot], s.clear_radius);
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

/// The big-message movie GTA's "WASTED" screen uses.
const BIG_MESSAGE: &str = "MP_BIG_MESSAGE_FREEMODE";
/// GTA's plain desaturation grade (the one behind the weapon wheel), used
/// at `@wreck_desat` strength for a Burnout-style washed-out look; and the
/// camera shake of GTA's own death screen.
const DESAT: &str = "hud_def_desat_Neutral";
const DEATH_SHAKE: &str = "DEATH_FAIL_IN_EFFECT_SHAKE";

/// GTA's own "WASTED" screen showing another word: the same big message
/// (font, size, place), sound and camera shake, with the colours washed
/// out. The movie is loaded when the plugin starts, so it's ready the moment
/// a wreck happens.
struct Banner {
    word: &'static str,
    movie: i32,
    desat: f32,
    shown: bool,
    since: std::time::Instant,
}

impl Banner {
    fn new(word: &'static str, movie: i32, desat: f32) -> Self {
        Banner { word, movie, desat, shown: false, since: std::time::Instant::now() }
    }

    /// Every frame while it's up; `cam` is the crash camera (0: gameplay).
    fn draw(&mut self, cam: i32) {
        if !self.shown && n::has_scaleform_movie_loaded(self.movie) {
            // ~r~: GTA's red, as its own "wasted" is.
            n::scaleform_call(self.movie, "SHOW_SHARD_WASTED_MP_MESSAGE", &[&format!("~r~{}", self.word), ""]);
            n::play_sound_frontend("ScreenFlash", "WastedSounds");
            if self.desat > 0.0 {
                n::set_timecycle_modifier(DESAT, self.desat.min(1.0));
            }
            if cam != 0 {
                n::shake_cam(cam, DEATH_SHAKE, 1.0);
            } else {
                n::shake_gameplay_cam(DEATH_SHAKE, 1.0);
            }
            self.shown = true;
        }
        if self.shown {
            n::draw_scaleform_fullscreen(self.movie);
        } else if self.since.elapsed().as_millis() > 500 {
            // Still not loaded: plain text rather than nothing.
            n::draw_text(self.word, 0.40, 0.40, 1.4, [255, 60, 30, 255]);
        }
    }

    fn finish(self) {
        if self.desat > 0.0 {
            n::clear_timecycle_modifier();
        }
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
    /// A vehicle never counted (the police car that took the player down),
    /// nor anyone in it.
    spare: i32,
}

impl Involved {
    fn add_vehicle(&mut self, v: i32) {
        if v != self.spare && !self.vehicles.contains(&v) {
            self.vehicles.push(v);
        }
    }

    fn add_ped(&mut self, p: i32) {
        if !in_vehicle(p, self.spare) && !self.peds.contains(&p) {
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

/// Whether `p` is in vehicle `v` (0: never).
fn in_vehicle(p: i32, v: i32) -> bool {
    v != 0 && n::is_ped_in_any_vehicle(p) && n::get_vehicle_ped_is_in(p) == v
}

/// Removes the NPCs and vehicles within `radius` of any of `spots` (except
/// the player, the vehicles in `keep`, and `spare` with anyone in it), and
/// puts out fires there. Returns how many were removed.
fn clear_around(ped: i32, keep: &[i32], spare: i32, spots: &[Vec3], radius: f32) -> usize {
    if radius <= 0.0 {
        return 0;
    }
    let spare = if spare != 0 && n::does_entity_exist(spare) { spare } else { 0 };
    let near = |e: i32| {
        let q = n::get_entity_coords(e);
        spots.iter().any(|at| (q.x - at.x).powi(2) + (q.y - at.y).powi(2) + (q.z - at.z).powi(2) <= radius * radius)
    };
    let vehicles: Vec<i32> = shv::all_vehicles().into_iter().filter(|&v| v != spare && !keep.contains(&v) && near(v)).collect();
    let mut removed = 0;
    // Peds first (including anyone in those vehicles), then the vehicles.
    for p in shv::all_peds() {
        if p == ped || n::is_ped_a_player(p) || !n::does_entity_exist(p) || in_vehicle(p, spare) {
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
    // player and the car they're in alone, but would take the spared police
    // car: not near it), and the explosion's fires go.
    let s = if spare != 0 { n::get_entity_coords(spare) } else { Vec3::default() };
    for &at in spots {
        if spare == 0 || (s.x - at.x).powi(2) + (s.y - at.y).powi(2) + (s.z - at.z).powi(2) > (radius + 5.0).powi(2) {
            n::clear_area_of_peds(at, radius);
            n::clear_area_of_vehicles(at, radius);
        }
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
    /// The driver, thrown out: framed together with the car (0: car only).
    follow: i32,
}

impl CrashCam {
    const RADIUS: f32 = 7.5;
    const HEIGHT: f32 = 1.6;
    /// Orbit speed, radians per real second.
    const SPIN: f32 = 0.35;

    fn new(enabled: bool, veh: i32, travel: f32) -> Self {
        if !enabled {
            return Self { cam: 0, angle: 0.0, follow: 0 };
        }
        let cam = n::create_cam();
        n::set_cam_fov(cam, 50.0);
        n::point_cam_at_entity(cam, veh);
        // Ahead and to the right of where the car was heading, looking back
        // at it as it comes in.
        let h = travel.to_radians();
        let (fx, fy) = (-h.sin(), h.cos());
        let angle = (fy * 0.6 - fx).atan2(fx * 0.6 + fy) ;
        let mut c = Self { cam, angle, follow: 0 };
        c.update(veh, 0.0);
        n::set_cam_active(cam, true);
        n::render_script_cams(true, 0);
        c
    }

    fn update(&mut self, veh: i32, t: f32) {
        if self.cam == 0 {
            return;
        }
        let car = n::get_entity_coords(veh);
        // The car, or the car and the thrown driver together: from between
        // them, further back the further apart they are.
        let (p, radius, height) = if self.follow != 0 && n::does_entity_exist(self.follow) {
            let d = n::get_entity_coords(self.follow);
            let apart = ((d.x - car.x).powi(2) + (d.y - car.y).powi(2) + (d.z - car.z).powi(2)).sqrt();
            let mid = Vec3::new((car.x + d.x) * 0.5, (car.y + d.y) * 0.5, (car.z + d.z) * 0.5);
            n::point_cam_at_coord(self.cam, mid);
            (mid, Self::RADIUS.max(apart * 0.8 + 4.0), Self::HEIGHT + apart * 0.12)
        } else {
            (car, Self::RADIUS, Self::HEIGHT)
        };
        let a = self.angle + t * Self::SPIN;
        let want = Vec3::new(p.x + a.cos() * radius, p.y + a.sin() * radius, p.z + height);
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

/// Being thrown out only counts as a wreck from this speed (m/s), with the
/// car's speed changing suddenly (not just bailing out of a moving car).
const EJECT_MIN_SPEED: f32 = 12.0;

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
    // Looks only: the wheels stay where the crash itself left them.
    n::set_vehicle_can_deform_wheels(veh, false);
    let (x, y) = local;
    for k in 0..hits {
        // Fan the hits across the struck face.
        let spread = (k as f32 / (hits.max(2) - 1) as f32 - 0.5) * 1.6;
        let (px, py) = (x * 1.0 + -y * spread * 0.6, y * 2.2 + x * spread);
        n::set_vehicle_damage(veh, px, py, 0.1 + (k % 2) as f32 * 0.3, damage, 1.2);
    }
    n::set_vehicle_can_deform_wheels(veh, true);
}

/// Puts engine and body health back up to what they were.
fn give_back_health(veh: i32, engine: f32, body: f32) {
    if n::get_vehicle_engine_health(veh) < engine {
        n::set_vehicle_engine_health(veh, engine);
    }
    if n::get_vehicle_body_health(veh) < body {
        n::set_vehicle_body_health(veh, body);
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
