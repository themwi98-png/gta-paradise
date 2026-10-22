//! Burnout-style boost for the player's car.
//!
//! The vehicle "duck" control (A on a controller, X on the keyboard) is taken
//! over: holding it while driving burns the boost gauge to push the car well
//! past its normal top speed, with exhaust flames, extra engine power (the
//! engine works harder and louder) and GTA's speed-boost sound. An emptied
//! gauge stays locked until the button is let go.
//! The gauge refills from driving like Burnout: going fast, driving in the
//! oncoming lane (or the wrong way down a one-way road), near misses (more
//! for oncoming traffic), drifting and air time. Settings: `@boost`,
//! `@boost_accel`, `@boost_top`, `@boost_min_top`, `@boost_max_top`,
//! `@boost_seconds`, `@boost_fill`.

use crate::natives as n;
use crate::shv;
use std::collections::HashMap;

/// `@boost...` settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct BoostSettings {
    pub enabled: bool,
    /// Extra acceleration while boosting (m/s²).
    pub accel: f32,
    /// Boosted top speed as a multiple of the car's own top speed...
    pub top_mult: f32,
    /// ...but at least this (m/s; 47 = 105 mph)...
    pub min_top: f32,
    /// ...and at most this (m/s; 95 = 212 mph).
    pub max_top: f32,
    /// Seconds a full gauge lasts.
    pub seconds: f32,
    /// Multiplier on how fast the gauge refills.
    pub fill: f32,
    /// Engine power while boosting is ENGINE_POWER times this (follows
    /// `@car_power`, so boosting is always a step up from not boosting).
    pub power: f32,
}

impl Default for BoostSettings {
    fn default() -> Self {
        Self { enabled: true, accel: 9.0, top_mult: 1.8, min_top: 47.0, max_top: 135.0, seconds: 15.0, fill: 1.5, power: 1.0 }
    }
}

/// INPUT_VEH_DUCK
const CONTROL_BOOST: i32 = 73;
const EXHAUSTS: [&str; 4] = ["exhaust", "exhaust_2", "exhaust_3", "exhaust_4"];
/// Engine torque multiplier while boosting (louder, harder-working engine).
const ENGINE_POWER: f32 = 1.8;
/// A car passing with its centre this close (m) to ours, side to side, is a
/// near miss (about 2.5 m of space between two ordinary cars).
const NEAR_MISS_SIDE: f32 = 4.5;

pub struct Boost {
    /// 0..1
    pub gauge: f32,
    boosting: bool,
    /// The gauge ran out: no boost until the button is released.
    locked: bool,
    last_flame: i32,
    air_since: Option<i32>,
    /// Nearby cars' last position along ours (local y), for near misses.
    passing: HashMap<i32, f32>,
    last_scan: i32,
    last_collision: i32,
    last_road: i32,
    /// On the wrong side of the road (checked a few times a second).
    oncoming: bool,
    /// Message shown under the gauge, and until when.
    note: Option<(String, i32)>,
    vehicle: i32,
}

impl Default for Boost {
    fn default() -> Self {
        Self { gauge: 0.5, boosting: false, locked: false, last_flame: 0, air_since: None, passing: HashMap::new(), last_scan: 0, last_collision: -10_000, last_road: 0, oncoming: false, note: None, vehicle: 0 }
    }
}

impl Boost {
    fn award(&mut self, s: &BoostSettings, amount: f32, what: &str, now: i32) {
        self.gauge = (self.gauge + amount * s.fill).min(1.0);
        if !what.is_empty() {
            self.note = Some((what.to_string(), now + 1200));
        }
    }

    /// A reward from elsewhere (takedown.rs): `amount` of the gauge as it
    /// is (not scaled by `@boost_fill`), with a note under the gauge unless
    /// `what` is empty.
    pub fn reward(&mut self, amount: f32, what: &str, now: i32) {
        self.gauge = (self.gauge + amount).min(1.0);
        if !what.is_empty() {
            self.note = Some((what.to_string(), now + 1200));
        }
    }

    /// Takes `amount` of the gauge away (taken down).
    pub fn lose(&mut self, amount: f32) {
        self.gauge = (self.gauge - amount).max(0.0);
    }

    pub fn is_boosting(&self) -> bool {
        self.boosting
    }

    pub fn stop(&mut self) {
        if self.boosting {
            if self.vehicle != 0 {
                n::set_vehicle_boost_active(self.vehicle, false);
            }
            self.boosting = false;
        }
    }

    pub fn update(&mut self, s: &BoostSettings, ped: i32, now: i32) {
        if !s.enabled || !n::is_ped_in_any_vehicle(ped) {
            self.stop();
            return;
        }
        let veh = n::get_vehicle_ped_is_in(ped);
        if matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) || n::get_ped_in_vehicle_seat(veh, -1) != ped {
            self.stop();
            return;
        }
        if veh != self.vehicle {
            self.stop();
            self.vehicle = veh;
            self.passing.clear();
            self.air_since = None;
        }
        n::disable_control(CONTROL_BOOST);
        let dt = n::get_frame_time().clamp(0.0, 0.1);
        let local = n::get_entity_speed_vector_relative(veh);
        let speed = n::get_entity_speed(veh);
        let in_air = n::is_entity_in_air(veh);
        if n::has_entity_collided_with_anything(veh) {
            self.last_collision = now;
        }

        // --- Boosting ---
        let held = n::is_disabled_control_pressed(CONTROL_BOOST);
        if !held {
            self.locked = false;
        }
        let want = held && !self.locked && self.gauge > 0.0 && local.y > -1.0;
        if want && !self.boosting {
            n::set_vehicle_boost_active(veh, true);
            self.boosting = true;
        } else if !want {
            self.stop();
        }
        if self.boosting {
            n::set_vehicle_cheat_power_increase(veh, ENGINE_POWER * s.power.max(1.0));
            self.gauge = (self.gauge - dt / s.seconds.max(0.5)).max(0.0);
            if self.gauge <= 0.0 {
                self.locked = true;
            }
            let top = (n::get_vehicle_estimated_max_speed(veh) * s.top_mult).clamp(s.min_top, s.max_top);
            if !in_air && local.y < top {
                // Push along the car's nose; fades out approaching the top.
                let a = s.accel * (1.0 - (local.y / top).max(0.0).powi(4)) * dt;
                let f = n::get_entity_forward_vector(veh);
                let v = n::get_entity_velocity(veh);
                n::set_entity_velocity(veh, v.x + f.x * a, v.y + f.y * a, v.z + f.z * a);
            }
            if now - self.last_flame > 45 {
                self.last_flame = now;
                if n::request_ptfx_asset("core") {
                    for b in EXHAUSTS {
                        let bone = n::get_entity_bone_index_by_name(veh, b);
                        if bone >= 0 {
                            n::ptfx_on_bone("core", "veh_backfire", veh, bone, 1.4);
                        }
                    }
                }
            }
        }

        // --- Earning boost ---
        // Going fast.
        if !self.boosting && local.y > 25.0 {
            self.award(s, dt * 0.03 * ((local.y - 25.0) / 20.0).min(1.5), "", now);
        }
        // Oncoming lane.
        if now - self.last_road >= 200 {
            self.last_road = now;
            self.oncoming = !in_air && local.y > 12.0 && wrong_side(veh);
        }
        if self.oncoming {
            self.award(s, dt * 0.08, "ONCOMING", now);
        }
        // Drifting: sliding sideways at speed.
        if !in_air && local.x.abs() > 4.0 && local.y > 12.0 {
            self.award(s, dt * 0.12, "DRIFT", now);
        }
        // Air time, with a bonus for a big jump.
        match (in_air && speed > 10.0, self.air_since) {
            (true, None) => self.air_since = Some(now),
            (true, Some(_)) => self.award(s, dt * 0.15, "", now),
            (false, Some(t)) => {
                self.air_since = None;
                if now - t > 800 {
                    self.award(s, 0.1, "BIG AIR", now);
                }
            }
            _ => {}
        }
        // Near misses: a car going by within a few metres, without touching.
        if now - self.last_scan >= 33 {
            self.last_scan = now;
            self.near_misses(s, veh, local.y, now);
        }

        self.draw(now);
    }

    fn near_misses(&mut self, s: &BoostSettings, veh: i32, my_speed: f32, now: i32) {
        if my_speed < 15.0 {
            self.passing.clear();
            return;
        }
        let me = n::get_entity_coords(veh);
        let f = n::get_entity_forward_vector(veh);
        let (fx, fy) = (f.x, f.y);
        let (rx, ry) = (fy, -fx);
        let my_v = n::get_entity_velocity(veh);
        let mut seen = HashMap::new();
        for other in shv::all_vehicles() {
            if other == veh {
                continue;
            }
            let p = n::get_entity_coords(other);
            let (dx, dy, dz) = (p.x - me.x, p.y - me.y, p.z - me.z);
            if dx * dx + dy * dy > 30.0 * 30.0 || dz.abs() > 4.0 {
                continue;
            }
            let along = dx * fx + dy * fy;
            let side = dx * rx + dy * ry;
            if let Some(&before) = self.passing.get(&other) {
                // Passed from in front to behind, close alongside.
                if before > 0.0 && along <= 0.0 && side.abs() < NEAR_MISS_SIDE && now - self.last_collision > 600 {
                    let v = n::get_entity_velocity(other);
                    let oncoming = v.x * my_v.x + v.y * my_v.y < 0.0;
                    if oncoming {
                        self.award(s, 0.12, "ONCOMING NEAR MISS", now);
                    } else {
                        self.award(s, 0.07, "NEAR MISS", now);
                    }
                }
            }
            seen.insert(other, along);
        }
        self.passing = seen;
    }

    fn draw(&mut self, now: i32) {
        // Gauge, bottom right.
        let (x, y, w, h) = (0.86, 0.93, 0.2, 0.018);
        n::draw_rect(x, y, w + 0.006, h + 0.008, [0, 0, 0, 160]);
        let fill = w * self.gauge;
        let colour = if self.boosting { [255, 140, 20, 240] } else { [60, 170, 255, 230] };
        if fill > 0.0 {
            n::draw_rect(x - w / 2.0 + fill / 2.0, y, fill, h, colour);
        }
        n::draw_text("BOOST", x - w / 2.0, y - 0.045, 0.45, [255, 255, 255, 230]);
        if let Some((text, until)) = &self.note {
            if now < *until {
                n::draw_text(text, x - w / 2.0, y - 0.085, 0.5, [255, 200, 40, 255]);
            } else {
                self.note = None;
            }
        }
    }
}

/// Is the car driving against the traffic on its road? On a two-way road the
/// road's nodes run along the line between the two directions, and traffic
/// keeps right of it; on a one-way road, any driving against it counts.
fn wrong_side(veh: i32) -> bool {
    let p = n::get_entity_coords(veh);
    let Some((a, b, fwd, back)) = n::closest_road(p) else { return false };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return false;
    }
    let (ux, uy) = (dx / len, dy / len);
    // Close to the road (not across a junction or on a parallel street).
    let (px, py) = (p.x - a.x, p.y - a.y);
    let side = px * uy - py * ux; // + = right of a -> b
    let t = px * ux + py * uy;
    if side.abs() > 14.0 || t < -10.0 || t > len + 10.0 || (p.z - a.z).abs() > 6.0 {
        return false;
    }
    let v = n::get_entity_velocity(veh);
    // GET_CLOSEST_ROAD's lane counts are for b -> a on Paradise's roads (in
    // game, the a -> b reading said "oncoming" with the flow of traffic).
    let along = v.x * ux + v.y * uy < 0.0;
    match (fwd > 0, back > 0) {
        (true, true) => (along && side < -1.0) || (!along && side > 1.0),
        (true, false) => !along,
        (false, true) => along,
        _ => false,
    }
}
