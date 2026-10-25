//! A race rival's driving (races.rs), Burnout style: any road either way,
//! fast, boosting, and back on the road after a wreck.
//!
//! - Route: gps.rs's road route to its next race point (one-way streets
//!   and the side of the road don't count), then the point itself.
//! - Steering: sent straight at the farthest route point it can reach
//!   without the line leaving the road (every route point on the way within
//!   LANE m of it, at most AIM_MAX m off) or hitting anything (a ray at
//!   bumper height against the map and props: medians, walls), with
//!   TASK_VEHICLE_DRIVE_TO_COORD in straight-line mode, so GTA's own lanes
//!   and one-way rules play no part. Sent on when near that point, when
//!   the way to it gets blocked, or every REAIM_MS. Route points just
//!   behind it when it sets off are skipped (no turning round at a start).
//! - Speed: as fast as the corners ahead allow, times `@race_rival_speed`
//!   (`speed`): each corner's speed from
//!   its radius (GRIP m/s² sideways), braking (BRAKE m/s²) from there back
//!   to the car; RIVAL_SPEED at most. A corner's radius is the circle
//!   through the route CORNER_SCALES m either side of a point and the point
//!   itself moved LANE m towards them (the slower of the scales): the car
//!   cuts that much anyway, so the road nodes jinking a few metres across a
//!   junction aren't taken for a bend. Set as the task's cruise speed, and
//!   held to every frame: over it, the car is braked at BRAKE m/s² whatever
//!   its driver does (super brakes, like the police's), never sliding.
//! - Boost: full at the start and burnt off the line (from a standstill);
//!   after that a gauge that fills while driving fast, burnt (boost.rs's push,
//!   the player's settings) on a straight with no corner coming that needs
//!   braking for, and the car pointing where it's going.
//! - The driver never gets out: GTA's reasons to leave a car (undrivable,
//!   burning, upside down, panic, combat, being dragged out) are switched
//!   off, and a driver found out of the seat or climbing out is put
//!   straight back in (logged).
//! - Wrecked (car undrivable or gone, driver dead or out of the car) or
//!   stuck (under STUCK_MOVE m in `stuck_ms`, `@race_stuck`): put back on
//!   its route `respawn_ms` (`@race_respawn`) later, repaired (a new car or driver if they're gone).
//! - Off its route (a wrong turn: over OFF_ROUTE m from it), or with no
//!   clear line to the route ahead: straight for the furthest route point
//!   ahead it has a clear line to, over kerbs, through car parks; with none
//!   in sight for REPLAN_MS, a new route from where it is.
//! - Catch-up (races.rs decides when): moved on along its route, out of the
//!   player's sight, keeping its speed.
//! - Grip: the police's (chase.rs): on the ground it goes where it
//!   points, all sideways sliding turned into speed along its nose each
//!   frame, and its turning held to what TURN_G allows at its speed, so it
//!   can't spin. Touched by the player's car, it's left to GTA's physics
//!   for HIT_GRACE ms, so it can still be pitted.
//! - Props: the player's prop shield and sweep (propshield.rs), so a race
//!   is an even match.
//! - Heights count (double) wherever it's matched to its route, so a road
//!   in a tunnel under the street isn't taken for the street.
//! - Map: an arrow (blip with a heading cone) in its own colour, numbered
//!   as in the log.
//! - Logged: wrecks and where it was put back, boosts, and every REPORT_MS
//!   where it is, how fast, its speed limit and gauge.

use crate::boost::{self, BoostSettings};
use crate::gps::GpsWatch;
use crate::natives as n;
use crate::propshield::{CarShield, PropShield};
use crate::shv::Vec3;

/// Driving style: swerve around cars (4), steer around stationary cars (8)
/// and objects (32), use the oncoming side (512), straight at the point
/// given (16777216).
const STYLE: i32 = 4 | 8 | 32 | 512 | 16_777_216;
/// The task drives straight at its point from this far (m): always.
const STRAIGHT: f32 = 10_000.0;
const RIVAL_SPEED: f32 = 70.0;
const MIN_SPEED: f32 = 8.0;
/// Sideways grip and braking the speed limit assumes (m/s²); BRAKE is
/// what it gets (frame), whatever its driver does, so ~2 g.
const GRIP: f32 = 13.0;
const BRAKE: f32 = 20.0;
/// Braked once this much (m/s) over the corner speed ahead.
const BRAKE_SLACK: f32 = 1.0;
/// Corners this far ahead (m) are looked at.
const LOOK_AHEAD: f32 = 300.0;
/// A corner's radius is measured over this much route (m) either side,
/// at least CORNER_MIN m near the route's ends.
const CORNER_SCALES: [f32; 2] = [30.0, 60.0];
const CORNER_MIN: f32 = 15.0;
/// Steering: farthest route point AIM_MAX m away whose line keeps every
/// route point on the way within LANE m; at least AIM_MIN m away.
const AIM_MAX: f32 = 90.0;
const AIM_MIN: f32 = 12.0;
const LANE: f32 = 3.5;
/// The blocked-way ray: this high (m) above the car's centre and the
/// road, against the map and props.
const RAY_HEIGHT: f32 = 0.2;
const RAY_ROAD_HEIGHT: f32 = 0.7;
const RAY_FLAGS: i32 = 1 | 16;
/// Route points this close (m) behind it when it sets off are skipped.
const BEHIND: f32 = 30.0;
/// Further than this (m, heights double) from its route is off it; with no
/// route point in sight for REPLAN_MS, a new route.
const OFF_ROUTE: f32 = 12.0;
const REPLAN_MS: i32 = 1500;
/// Grip and brakes only work above RAILS_SPEED (m/s) forward, all wheels
/// down; turning never pulls harder sideways than TURN_G (g, the
/// police's); after the player's car touches it, none for HIT_GRACE ms.
const RAILS_SPEED: f32 = 8.0;
const TURN_G: f32 = 3.0;
const G: f32 = 9.81;
const HIT_GRACE: i32 = 1500;
/// Sent on within REAIM m of its point, or after REAIM_MS.
const REAIM: f32 = 15.0;
const REAIM_MS: i32 = 1000;
/// Where it is on the route: the nearest of the next CATCH_UP points.
const CATCH_UP: usize = 15;
const STUCK_MOVE: f32 = 10.0;
/// A catch-up move never ends nearer the player than this (m).
const UNSEEN: f32 = 120.0;
/// Put back no closer than this (m) to the player.
const CLEAR_OF_PLAYER: f32 = 15.0;
/// Boost gauge: full after BOOST_FILL_S s at FAST m/s or more; burnt from
/// BOOST_START full; only with a corner speed BOOST_MARGIN m/s above the
/// car's speed and the car pointing within BOOST_ANGLE° of its point. Once
/// going it keeps on until the margin is half that and the angle half as
/// wide again (no flicking on and off).
const BOOST_FILL_S: f32 = 10.0;
const FAST: f32 = 20.0;
const BOOST_START: f32 = 0.15;
const BOOST_MARGIN: f32 = 6.0;
const BOOST_ANGLE: f32 = 20.0;
const REPORT_MS: i32 = 5000;
/// Map arrows: blip colours by rival (red, blue, green, orange, purple,
/// pink, light blue, dark green).
const BLIP_COLOURS: [i32; 8] = [1, 3, 2, 17, 27, 8, 38, 25];

fn flat(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Distance (m) with heights counting double: a road above or below isn't
/// near.
fn apart(a: Vec3, b: Vec3) -> f32 {
    flat(a, b).hypot(2.0 * (a.z - b.z))
}

/// How far (m) `p` is from the line a-b, heights counting double.
fn off_line(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = (dx * dx + dy * dy).max(1e-6);
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    let z = a.z + (b.z - a.z) * t;
    (a.x + dx * t - p.x).hypot(a.y + dy * t - p.y).hypot(2.0 * (z - p.z))
}

pub struct Rival {
    pub car: i32,
    pub ped: i32,
    /// Its car's model (for a new one after a wreck).
    model: u32,
    /// Its arrow on the map.
    blip: i32,
    /// Index of the race point it's driving to.
    pub next: usize,
    /// Race time it finished in (ms).
    pub done: Option<i32>,
    /// Its way there: road points, then the race point; how far along it
    /// each point is (m, flat) and the corner speed there (m/s, infinite on
    /// a straight).
    path: Vec<Vec3>,
    along: Vec<f32>,
    corners: Vec<f32>,
    /// Where it is on the path, the path point it was sent to, and when.
    step: usize,
    aim: Option<usize>,
    aimed_at: i32,
    /// The cruise speed last set.
    limit: f32,
    /// Where it was and when, to tell it's getting nowhere.
    progress: Option<(Vec3, i32)>,
    /// Wrecked since (waiting to be put back), and why.
    wrecked: Option<(i32, String)>,
    /// Off its route since (no route point in sight).
    off_since: Option<i32>,
    off_logged: bool,
    shield: CarShield,
    pub wrecks: u32,
    gauge: f32,
    boosting: bool,
    /// Off the line: boosting from a standstill until it's going FAST.
    launching: bool,
    /// Its speed limits, times (`@race_rival_speed`).
    pub speed: f32,
    /// Stuck after this long (ms) without moving STUCK_MOVE m
    /// (`@race_stuck`); put back this long (ms) after a wreck or getting
    /// stuck (`@race_respawn`).
    pub stuck_ms: i32,
    pub respawn_ms: i32,
    /// When its driver was last put back in the seat (logged once a second).
    reseated_at: i32,
    last_flame: i32,
    last_report: i32,
    /// Braking hard for a corner since this speed (m/s).
    braking: Option<f32>,
    /// When the player's car last touched it.
    hit_at: Option<i32>,
}

pub enum Event {
    /// Through race point `next - 1` (not the last).
    Checkpoint,
    Finished,
}

impl Rival {
    pub fn new(car: i32, ped: i32, model: u32) -> Self {
        Self {
            car,
            ped,
            model,
            blip: 0,
            next: 0,
            done: None,
            path: Vec::new(),
            along: Vec::new(),
            corners: Vec::new(),
            step: 0,
            aim: None,
            aimed_at: 0,
            limit: 0.0,
            progress: None,
            wrecked: None,
            off_since: None,
            off_logged: false,
            shield: CarShield::default(),
            wrecks: 0,
            gauge: 0.0,
            boosting: false,
            launching: false,
            speed: 1.0,
            stuck_ms: 6000,
            respawn_ms: 2500,
            reseated_at: i32::MIN / 2,
            last_flame: 0,
            braking: None,
            hit_at: None,
            last_report: 0,
        }
    }

    /// Its arrow on the map, on its (current) car.
    pub fn mark(&mut self, k: usize) {
        self.unmark();
        self.blip = n::add_blip_for_entity(self.car);
        n::set_blip_sprite(self.blip, 1);
        n::set_blip_colour(self.blip, BLIP_COLOURS[k % BLIP_COLOURS.len()]);
        n::show_heading_indicator_on_blip(self.blip, true);
        n::show_number_on_blip(self.blip, k as i32 + 1);
        n::set_blip_scale(self.blip, 0.9);
        n::set_blip_name(self.blip, &format!("Rival {}", k + 1));
    }

    pub fn unmark(&mut self) {
        if self.blip != 0 {
            n::remove_blip(self.blip);
            self.blip = 0;
        }
    }

    pub fn blip(&self) -> i32 {
        self.blip
    }

    /// The driver's settings for racing.
    pub fn set_up_driver(ped: i32) {
        n::set_entity_as_mission_entity(ped);
        n::set_blocking_of_non_temporary_events(ped, true);
        n::set_ped_keep_task(ped, true);
        n::set_driver_ability(ped, 1.0);
        n::set_driver_aggressiveness(ped, 1.0);
        n::set_driver_racing_modifier(ped, 1.0);
        // No getting out, whatever happens to the car.
        n::set_ped_config_flag(ped, 29, false); // GetOutUndriveableVehicle
        n::set_ped_config_flag(ped, 116, false); // GetOutBurningVehicle
        n::set_ped_config_flag(ped, 118, false); // RunFromFiresAndExplosions
        n::set_ped_config_flag(ped, 229, true); // DisablePanicInVehicle
        n::set_ped_config_flag(ped, 430, true); // IgnoreBeingOnFire
        n::set_ped_config_flag(ped, 32, false); // WillFlyThroughWindscreen
        n::set_ped_combat_attributes(ped, 3, false); // CanLeaveVehicle
        n::set_ped_get_out_upside_down_vehicle(ped, false);
        n::set_ped_can_be_dragged_out(ped, false);
        n::set_ped_stay_in_vehicle_when_jacked(ped, true);
        n::set_ped_can_be_knocked_off_vehicle(ped, 1); // never (bikes)
    }

    /// A driver out of the seat or climbing out (alive, car drivable) is
    /// put straight back in and sent on again.
    fn keep_seated(&mut self, k: usize, now: i32) {
        if !n::does_entity_exist(self.ped) || n::is_entity_dead(self.ped) || n::is_entity_dead(self.car) || !n::is_vehicle_driveable(self.car) {
            return;
        }
        let out = n::get_ped_in_vehicle_seat(self.car, -1) != self.ped;
        if !out && !n::get_is_task_active(self.ped, 2) {
            return;
        }
        n::set_ped_into_vehicle(self.ped, self.car);
        self.aim = None;
        self.steer_later(now);
        if now - self.reseated_at > 1000 {
            let at = n::get_entity_coords(self.car);
            crate::log(&format!(
                "races: rival {}'s driver {} at ({:.0}, {:.0}); put back in the seat",
                k + 1,
                if out { "was out of the car" } else { "was getting out" },
                at.x,
                at.y
            ));
        }
        self.reseated_at = now;
    }

    /// Its way to `target` (its next race point): the roads, then the
    /// point; straight at it if there's no road route.
    pub fn plan(&mut self, k: usize, target: Vec3, gps: &mut GpsWatch) {
        let at = n::get_entity_coords(self.car);
        self.path = match gps.road_path(at, n::get_entity_heading(self.car), target) {
            Ok(p) => p,
            Err(e) => {
                crate::log(&format!("races: rival {} has no road route to ({:.0}, {:.0}): {e}; driving straight at it", k + 1, target.x, target.y));
                Vec::new()
            }
        };
        self.path.push(target);
        // Not back to points just behind it.
        let f = n::get_entity_forward_vector(self.car);
        let skip = self.path[..self.path.len() - 1].iter().take_while(|p| flat(at, **p) < BEHIND && (p.x - at.x) * f.x + (p.y - at.y) * f.y < 0.0).count();
        self.path.drain(..skip);
        self.step = 0;
        self.aim = None;
        self.find_corners(k);
    }

    /// Each route point's corner speed (see the top of the file).
    fn find_corners(&mut self, k: usize) {
        let path = &self.path;
        self.along = std::iter::once(0.0)
            .chain(path.windows(2).scan(0.0, |d, w| {
                *d += flat(w[0], w[1]);
                Some(*d)
            }))
            .collect();
        let along = &self.along;
        let total = along.last().copied().unwrap_or(0.0);
        // The route point (flat) `s` m along.
        let at = |s: f32| {
            let s = s.clamp(0.0, total);
            let i = along.partition_point(|&d| d <= s).clamp(1, path.len().max(2) - 1);
            let (a, b) = (path[i - 1], path[i]);
            let t = if along[i] > along[i - 1] { (s - along[i - 1]) / (along[i] - along[i - 1]) } else { 0.0 };
            (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
        };
        self.corners = (0..path.len())
            .map(|j| {
                let p = path[j];
                let mut speed = f32::INFINITY;
                for scale in CORNER_SCALES {
                    let d = scale.min(along[j]).min(total - along[j]);
                    if d < CORNER_MIN {
                        continue;
                    }
                    let (a, c) = (at(along[j] - d), at(along[j] + d));
                    let (dx, dy) = (c.0 - a.0, c.1 - a.1);
                    let chord = dx.hypot(dy);
                    if chord < 1.0 {
                        continue;
                    }
                    let sag = (dx * (p.y - a.1) - dy * (p.x - a.0)).abs() / chord - LANE;
                    if sag <= 0.0 {
                        continue;
                    }
                    let h = chord / 2.0;
                    speed = speed.min((GRIP * (h * h + sag * sag) / (2.0 * sag)).sqrt());
                }
                speed
            })
            .collect();
        if let Some((j, v)) = self.corners.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).filter(|(_, v)| v.is_finite()) {
            let slow = self.corners.iter().filter(|&&v| v < RIVAL_SPEED * 0.6).count();
            crate::log(&format!(
                "races: rival {} route {:.0} m, {} point(s) slower than {:.0} m/s, slowest {v:.0} m/s at ({:.0}, {:.0})",
                k + 1,
                total,
                slow,
                RIVAL_SPEED * 0.6,
                self.path[j].x,
                self.path[j].y
            ));
        }
    }

    /// Nothing solid on the straight line from `at` (its centre) to route
    /// point `p`.
    fn clear(&self, at: Vec3, p: Vec3) -> bool {
        let from = Vec3::new(at.x, at.y, at.z + RAY_HEIGHT);
        let to = Vec3::new(p.x, p.y, p.z + RAY_ROAD_HEIGHT);
        n::raycast(from, to, RAY_FLAGS, self.car).is_none()
    }

    /// Every RIVAL_MS while racing: wrecks, reaching race points, steering
    /// and speed. `points`: the race's points; `player`: where the player is.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, k: usize, points: &[Vec3], clock: i32, player: Vec3, driver_model: u32, gps: &mut GpsWatch, now: i32) -> Option<Event> {
        if self.done.is_some() {
            return None;
        }
        if let Some((since, why)) = self.wrecked.clone() {
            if now - since >= self.respawn_ms && self.put_back(k, player, driver_model, gps, now) {
                crate::log(&format!("races: rival {} back on the road after wreck {} ({why})", k + 1, self.wrecks));
                self.wrecked = None;
            }
            return None;
        }
        if let Some(why) = self.wreck_check(now) {
            self.wrecks += 1;
            self.boosting = false;
            PropShield::drop_rival(&mut self.shield);
            let at = if n::does_entity_exist(self.car) { n::get_entity_coords(self.car) } else { Vec3::default() };
            crate::log(&format!("races: rival {} wrecked ({why}) at ({:.0}, {:.0}, {:.0}); back on its route in {:.1} s", k + 1, at.x, at.y, at.z, self.respawn_ms as f32 / 1000.0));
            self.wrecked = Some((now, why));
            return None;
        }
        let at = n::get_entity_coords(self.car);
        let target = points[self.next];
        if flat(at, target) <= crate::races::RIVAL_REACH && (at.z - target.z).abs() <= crate::races::REACH_HEIGHT {
            self.next += 1;
            if self.next == points.len() {
                self.done = Some(clock);
                self.stop_boost();
                return Some(Event::Finished);
            }
            self.plan(k, points[self.next], gps);
            self.steer(k, at, gps, now);
            return Some(Event::Checkpoint);
        }
        self.steer(k, at, gps, now);
        if now - self.last_report >= REPORT_MS {
            self.last_report = now;
            crate::log(&format!(
                "races: rival {} at ({:.0}, {:.0}) {:.0} m/s, limit {:.0}, route point {}/{}, gauge {:.0}%{}",
                k + 1,
                at.x,
                at.y,
                n::get_entity_speed(self.car),
                self.limit,
                self.step + 1,
                self.path.len(),
                self.gauge * 100.0,
                if self.boosting { ", boosting" } else { "" }
            ));
        }
        None
    }

    /// Why it's out of the running, if it is.
    fn wreck_check(&mut self, now: i32) -> Option<String> {
        let why = if !n::does_entity_exist(self.car) {
            "car gone"
        } else if n::is_entity_dead(self.car) || !n::is_vehicle_driveable(self.car) {
            "car wrecked"
        } else if !n::does_entity_exist(self.ped) || n::is_entity_dead(self.ped) {
            "driver dead or gone"
        } else if n::get_ped_in_vehicle_seat(self.car, -1) != self.ped {
            "driver out of the car"
        } else {
            // Getting nowhere (stopped, or bumping a wall back and forth).
            let at = n::get_entity_coords(self.car);
            let (from, since) = *self.progress.get_or_insert((at, now));
            if flat(at, from) >= STUCK_MOVE {
                self.progress = Some((at, now));
            }
            if flat(at, from) >= STUCK_MOVE || now - since < self.stuck_ms {
                return None;
            }
            "stuck"
        };
        self.progress = None;
        Some(why.to_string())
    }

    /// Back onto its route where it got to (further on if that's next to
    /// the player), repaired; a new car or driver if gone. False: the
    /// models aren't loaded yet (tried again next time).
    fn put_back(&mut self, k: usize, player: Vec3, driver_model: u32, gps: &mut GpsWatch, now: i32) -> bool {
        let car_model = self.model;
        let mut i = self.step.min(self.path.len() - 1);
        while i + 1 < self.path.len() && flat(self.path[i], player) < CLEAR_OF_PLAYER {
            i += 1;
        }
        let at = self.path[i];
        let heading = self.heading_at(i);
        let car_ok = n::does_entity_exist(self.car) && !n::is_entity_dead(self.car);
        let driver_ok = car_ok && n::does_entity_exist(self.ped) && !n::is_entity_dead(self.ped);
        if !car_ok || !driver_ok {
            n::request_model(car_model);
            n::request_model(driver_model);
            if !n::has_model_loaded(car_model) || !n::has_model_loaded(driver_model) {
                return false;
            }
        }
        let spot = Vec3::new(at.x, at.y, at.z + 0.5);
        if car_ok {
            n::repair_vehicle(self.car);
            n::set_entity_coords(self.car, spot);
        } else {
            if n::does_entity_exist(self.ped) {
                n::delete_ped(self.ped);
            }
            if n::does_entity_exist(self.car) {
                n::delete_vehicle(self.car);
            }
            self.car = n::create_vehicle(car_model, spot, heading);
            if self.car == 0 {
                return false;
            }
            n::set_entity_as_mission_entity(self.car);
            self.mark(k);
        }
        n::set_entity_heading(self.car, heading);
        n::set_vehicle_on_ground_properly(self.car);
        n::set_entity_velocity(self.car, 0.0, 0.0, 0.0);
        n::set_vehicle_engine_on(self.car);
        if !driver_ok || n::get_ped_in_vehicle_seat(self.car, -1) != self.ped {
            if n::does_entity_exist(self.ped) {
                n::delete_ped(self.ped);
            }
            self.ped = n::create_ped_inside_vehicle(self.car, 4, driver_model, -1);
            if self.ped == 0 {
                return false;
            }
            Self::set_up_driver(self.ped);
        }
        crate::log(&format!(
            "races: rival {} put back at route point {}/{} ({:.0}, {:.0}, {:.0}){}",
            k + 1,
            i + 1,
            self.path.len(),
            at.x,
            at.y,
            at.z,
            if car_ok { "" } else { " in a new car" }
        ));
        let target = *self.path.last().unwrap();
        self.plan(k, target, gps);
        self.limit = 0.0;
        self.progress = None;
        self.steer(k, spot, gps, now);
        true
    }

    /// The way its route goes at route point `i` (GTA heading).
    fn heading_at(&self, i: usize) -> f32 {
        let (at, toward) = (self.path[i], self.path.get(i + 1).copied().unwrap_or(self.path[i]));
        if flat(at, toward) > 0.5 {
            (-(toward.x - at.x)).atan2(toward.y - at.y).to_degrees().rem_euclid(360.0)
        } else {
            n::get_entity_heading(self.car)
        }
    }

    /// Catch-up: on along its route by up to `ahead` m (not past its next
    /// race point), somewhere at least UNSEEN m from the player, keeping its
    /// speed. The metres moved, or None if it can't be moved now.
    pub fn move_up(&mut self, k: usize, ahead: f32, player: Vec3, now: i32) -> Option<f32> {
        if self.done.is_some() || self.wrecked.is_some() || self.path.len() < 3 || !n::does_entity_exist(self.car) {
            return None;
        }
        let at = n::get_entity_coords(self.car);
        if flat(at, player) < UNSEEN {
            return None;
        }
        // The furthest point within `ahead`, short of the race point.
        let mut i = self.step;
        let mut gone = 0.0;
        while i + 2 < self.path.len() && gone + flat(self.path[i], self.path[i + 1]) <= ahead {
            gone += flat(self.path[i], self.path[i + 1]);
            i += 1;
        }
        while i > self.step && flat(self.path[i], player) < UNSEEN {
            gone -= flat(self.path[i - 1], self.path[i]);
            i -= 1;
        }
        if i <= self.step {
            return None;
        }
        let speed = n::get_entity_speed(self.car);
        let p = self.path[i];
        n::set_entity_coords(self.car, Vec3::new(p.x, p.y, p.z + 0.5));
        n::set_entity_heading(self.car, self.heading_at(i));
        n::set_vehicle_on_ground_properly(self.car);
        n::set_vehicle_forward_speed(self.car, speed);
        crate::log(&format!("races: rival {} moved up {gone:.0} m to route point {}/{} ({:.0}, {:.0}) at {speed:.0} m/s", k + 1, i + 1, self.path.len(), p.x, p.y));
        self.step = i;
        self.aim = None;
        self.progress = None;
        self.steer_later(now);
        Some(gone)
    }

    /// Steered again at its next update.
    fn steer_later(&mut self, now: i32) {
        self.aimed_at = now - REAIM_MS;
    }

    /// Wrecked, waiting to be put back.
    pub fn wrecked(&self) -> bool {
        self.wrecked.is_some()
    }

    /// Along its route: where it is on it, where it's sent, how fast.
    fn steer(&mut self, k: usize, at: Vec3, gps: &mut GpsWatch, now: i32) {
        let window = self.step..(self.step + CATCH_UP).min(self.path.len());
        if let Some(i) = window.min_by(|&a, &b| apart(at, self.path[a]).total_cmp(&apart(at, self.path[b]))) {
            self.step = i;
        }
        let last = self.path.len() - 1;
        let mut aim = self.step;
        let mut lost = apart(at, self.path[self.step]) > OFF_ROUTE;
        if !lost {
            // The farthest point whose line keeps to the road.
            for j in self.step + 1..=last {
                if flat(at, self.path[j]) > AIM_MAX || (self.step + 1..j).any(|m| off_line(self.path[m], at, self.path[j]) > LANE) {
                    break;
                }
                aim = j;
            }
            while aim < last && flat(at, self.path[aim]) < AIM_MIN {
                aim += 1;
            }
            // Nearer, while something's in the way.
            while aim > self.step + 1 && !self.clear(at, self.path[aim]) {
                aim -= 1;
            }
            // Even the next point blocked (nosed into a side road): as lost.
            lost = !self.clear(at, self.path[aim]);
        }
        if lost {
            // Off its route: straight for the furthest route point ahead in
            // sight, whatever's between (kerbs, car parks).
            let ahead = self.step..(self.step + 2 * CATCH_UP).min(self.path.len());
            match ahead.rev().find(|&j| flat(at, self.path[j]) <= AIM_MAX && self.clear(at, self.path[j])) {
                Some(j) => {
                    aim = j;
                    self.off_since = None;
                    if !self.off_logged {
                        self.off_logged = true;
                        crate::log(&format!(
                            "races: rival {} off its route ({:.0} m) or blocked at ({:.0}, {:.0}); straight for route point {}/{}",
                            k + 1,
                            apart(at, self.path[self.step]),
                            at.x,
                            at.y,
                            j + 1,
                            self.path.len()
                        ));
                    }
                }
                None => {
                    let since = *self.off_since.get_or_insert(now);
                    if now - since >= REPLAN_MS {
                        crate::log(&format!("races: rival {} off its route at ({:.0}, {:.0}) with none of it in sight; a new route from here", k + 1, at.x, at.y));
                        let target = *self.path.last().unwrap();
                        self.plan(k, target, gps);
                        self.off_since = None;
                        self.off_logged = false;
                        return;
                    }
                }
            }
        } else {
            self.off_since = None;
            self.off_logged = false;
        }
        let near = self.aim.is_none_or(|a| a >= self.path.len() || flat(at, self.path[a]) < REAIM || !self.clear(at, self.path[a]));
        let limit = self.corner_limit(at);
        if self.aim != Some(aim) && (near || now - self.aimed_at >= REAIM_MS) {
            self.aim = Some(aim);
            self.aimed_at = now;
            self.limit = limit;
            n::task_vehicle_drive_to_coord(self.ped, self.car, self.path[aim], limit, STYLE, 0.0, STRAIGHT);
        } else if (limit - self.limit).abs() >= 2.0 {
            self.limit = limit;
            n::set_drive_task_cruise_speed(self.ped, limit);
        }
    }

    /// The fastest it can go now and still make every corner ahead.
    fn corner_limit(&self, at: Vec3) -> f32 {
        let mut limit = RIVAL_SPEED;
        // From the route point it's at now (every frame, between updates).
        let mut i = self.step.min(self.path.len() - 1);
        while i + 1 < self.path.len() && apart(at, self.path[i + 1]) < apart(at, self.path[i]) {
            i += 1;
        }
        let d0 = flat(at, self.path[i]);
        for j in i..self.path.len().min(self.corners.len()) {
            let d = d0 + self.along[j] - self.along[i];
            if d > LOOK_AHEAD {
                break;
            }
            let corner = self.corners[j];
            if corner.is_finite() {
                limit = limit.min((corner * corner + 2.0 * BRAKE * d).sqrt());
            }
        }
        (limit * self.speed).max(MIN_SPEED)
    }

    /// At the go: a full gauge, burnt off the line.
    pub fn launch(&mut self, k: usize) {
        self.gauge = 1.0;
        self.launching = true;
        crate::log(&format!("races: rival {} starts with a full boost gauge", k + 1));
    }

    /// Every frame: props, grip and brakes, and the boost gauge and
    /// burning it. `player_car`: the player's car.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, k: usize, s: &BoostSettings, props: &mut PropShield, player_car: i32, dt: f32, now: i32) {
        if self.done.is_some() || self.wrecked.is_some() || !n::does_entity_exist(self.car) || self.path.is_empty() {
            self.stop_boost();
            return;
        }
        self.keep_seated(k, now);
        props.guard_rival(&mut self.shield, self.car, now);
        let limit = self.corner_limit(n::get_entity_coords(self.car));
        self.handling(k, limit, player_car, dt, now);
        if !s.enabled {
            self.stop_boost();
            return;
        }
        let speed = n::get_entity_speed(self.car);
        if !self.boosting && speed > FAST {
            self.gauge = (self.gauge + dt * s.fill / (BOOST_FILL_S * 1.5) * (speed / FAST).min(2.0)).min(1.0);
        }
        let pointing = self.aim.is_some_and(|a| {
            let p = self.path[a];
            let at = n::get_entity_coords(self.car);
            let f = n::get_entity_forward_vector(self.car);
            let (dx, dy) = (p.x - at.x, p.y - at.y);
            let cos = (f.x * dx + f.y * dy) / (f.x.hypot(f.y) * dx.hypot(dy)).max(1e-3);
            cos >= (BOOST_ANGLE * if self.boosting { 1.5 } else { 1.0 }).to_radians().cos()
        });
        let clear = limit >= speed + BOOST_MARGIN * if self.boosting { 0.5 } else { 1.0 };
        if self.launching && speed > FAST {
            self.launching = false;
        }
        let want = pointing && clear && (speed > MIN_SPEED || self.launching) && self.gauge > if self.boosting { 0.0 } else { BOOST_START };
        if want != self.boosting {
            self.boosting = want;
            crate::log(&format!(
                "races: rival {} {} boosting at {:.0} m/s (limit {:.0}, gauge {:.0}%){}",
                k + 1,
                if want { "starts" } else { "stops" },
                speed,
                limit,
                self.gauge * 100.0,
                if self.launching { ", off the line" } else { "" }
            ));
        }
        if self.boosting {
            self.gauge = (self.gauge - dt / s.seconds.max(0.5)).max(0.0);
            boost::push(s, self.car, dt, now, &mut self.last_flame);
        }
    }

    fn stop_boost(&mut self) {
        self.boosting = false;
    }

    /// The police's grip and brakes: going where it points (sliding turned
    /// into speed along the nose), turning held to TURN_G, and braked at
    /// BRAKE m/s² while over `limit`, the corner speed ahead.
    fn handling(&mut self, k: usize, limit: f32, player_car: i32, dt: f32, now: i32) {
        let car = self.car;
        if player_car != 0 && n::is_entity_touching_entity(car, player_car) {
            if self.hit_at.is_none_or(|t| now - t > HIT_GRACE) {
                crate::log(&format!("races: rival {} touched by the player's car: GTA's physics for {HIT_GRACE} ms", k + 1));
            }
            self.hit_at = Some(now);
        }
        let hit = self.hit_at.is_some_and(|t| now - t <= HIT_GRACE);
        if hit || dt <= 0.0 || n::is_entity_in_air(car) || !n::is_vehicle_on_all_wheels(car) || n::get_entity_speed_vector_relative(car).y < RAILS_SPEED {
            self.braked(k, None);
            return;
        }
        let (right, _, _) = n::get_entity_matrix(car);
        let v = n::get_entity_velocity(car);
        let side = v.x * right[0] + v.y * right[1];
        let speed = v.x.hypot(v.y);
        let (x, y) = (v.x - right[0] * side, v.y - right[1] * side);
        let mut want = speed;
        if speed > limit + BRAKE_SLACK {
            want = (speed - BRAKE * dt).max(limit);
            self.braking.get_or_insert(speed);
        } else {
            self.braked(k, Some(speed));
        }
        let keep = want / x.hypot(y).max(1e-3);
        n::set_entity_velocity(car, x * keep, y * keep, v.z);
        let w = n::get_entity_rotation_velocity(car);
        let max_yaw = TURN_G * G / want.max(1.0);
        if w.z.abs() > max_yaw {
            n::set_entity_angular_velocity(car, w.x, w.y, w.z.clamp(-max_yaw, max_yaw));
        }
    }

    /// Hard braking over (logged): at `speed` now, or cut short.
    fn braked(&mut self, k: usize, speed: Option<f32>) {
        if let Some(from) = self.braking.take() {
            match speed {
                Some(to) => crate::log(&format!("races: rival {} braked hard for a corner, {from:.0} to {to:.0} m/s", k + 1)),
                None => crate::log(&format!("races: rival {} braking hard from {from:.0} m/s cut short (off the ground, hit, or slow)", k + 1)),
            }
        }
    }

    /// The race is over for it: shield off.
    pub fn finish(&mut self) {
        PropShield::drop_rival(&mut self.shield);
    }

    /// A finished rival drives off like traffic.
    pub fn wander(&self) {
        n::task_vehicle_drive_wander(self.ped, self.car, 20.0, 786_603);
    }
}
