//! Burnout-style takedowns, on police cars.
//!
//! Burnout's own rules (its HUD messages and tuning names): every hit is
//! named first (slam, shunt...), and the car that was hit stays vulnerable
//! for a moment; a crash inside that time is the attacker's takedown, a
//! crash outside it the driver's own wreck.
//!
//! The frame the player's car first touches a police car, the hit is
//! classified from both cars' motion just before it: a shunt (nose into its
//! tail), slam (side to side), T-bone (nose into its side), vertical (coming
//! down on it from a jump) or head-on; and who hit whom (whose motion carried
//! it into the other). Under MIN_HIT m/s closing it's only a nudge.
//!
//! Police cars are the ones driven by police, and empty police models
//! (roadblocks). Until the player's car hits one it can't be damaged at all
//! (chase.rs, `@police_tough`), so police never take themselves out.
//!
//! A police car the player hit stays vulnerable for `@takedown_window` s
//! (half as long again after a T-bone or head-on, and for as long as the two
//! keep touching). If it crashes in that time (a sudden speed change of
//! `@takedown_speed` m/s or more against something other than the player's
//! car, rolled onto its side or roof, disabled, or spun round to a stop) it's
//! a TAKEDOWN!: VERTICAL or T-BONE after those hits, otherwise named after
//! what it crashed into (CAR / VAN / BUS TAKEDOWN!, TAKEDOWN INTO BIG-RIG!),
//! and DOUBLE / TRIPLE / MULTI for several within MULTI_MS. A hit of
//! `@takedown_smash` m/s closing or more takes it down there and then (an
//! empty one standing still: ROADBLOCK TAKEDOWN!). A takedown gives
//! `@takedown_boost` of the boost gauge, and the car is finished: its driver
//! loses control, it's thrown off the player's path (`@takedown_launch`), and
//! it blows up (`@takedown_explode`) once clear of the player, whose car and
//! driver are proofed against the blast for a moment. `@chase_takedowns`
//! police takedowns in one chase end it: YOU ESCAPED!, wanted level gone.
//!
//! With `@takedown_any` (until traffic checks exist) any vehicle near the
//! player can be taken down the same way, not only police; only police can
//! take the player down.
//!
//! The other way round, a police car ramming the player makes the player
//! vulnerable: a wreck inside the window (wreck.rs) is TAKEN DOWN (VERTICALLY
//! TAKEN DOWN when landed on) instead of WRECKED, and costs
//! `@taken_down_boost` of the gauge.
//!
//! As in Burnout the attacker is favoured: while the player's car is ramming
//! a police car (anything but head-on) the hit never wrecks it; it keeps
//! `@takedown_keep` of its speed and all its health (wreck.rs). Police cars
//! that are vulnerable or taken down are left to GTA's physics (no grip,
//! speed help or toughness from chase.rs).

use crate::boost::Boost;
use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::{HashMap, VecDeque};

/// `@takedown...` settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct TakedownSettings {
    pub enabled: bool,
    /// Seconds a police car stays vulnerable after the player's hit (and the
    /// player after a police car's).
    pub window: f32,
    /// Sudden change in speed (m/s) that is a vulnerable car's crash.
    pub crash_dv: f32,
    /// Share of the boost gauge a takedown gives.
    pub boost: f32,
    /// Blow taken-down cars up.
    pub explode: bool,
    /// Share of its speed the player's car keeps when it rams a police car.
    pub keep: f32,
    /// Share of the boost gauge lost when taken down.
    pub taken_boost: f32,
    /// A hit this hard (closing speed, m/s) takes a police car down at once
    /// (0: never).
    pub smash: f32,
    /// Any vehicle can be taken down, not only police.
    pub any: bool,
    /// A taken-down car is thrown off the player's path at this speed (m/s;
    /// 0: not).
    pub launch: f32,
    /// This many police takedowns in one chase end it (0: never).
    pub escape: u32,
    /// While ramming, the player's car can't be spun: it stays pointed and
    /// moving the way it was going (steering still turns it).
    pub straight: bool,
}

impl Default for TakedownSettings {
    fn default() -> Self {
        Self { enabled: true, window: 2.0, crash_dv: 10.0, boost: 0.5, explode: true, keep: 0.9, taken_boost: 0.25, smash: 20.0, any: true, launch: 14.0, escape: 5, straight: true }
    }
}

/// What the wreck check (wreck.rs) needs to know this frame.
#[derive(Clone, Copy, Default)]
pub struct Context {
    /// Takedowns are on: police cars going the player's way get rammed,
    /// not crashed into...
    pub rams_police: bool,
    /// ...and so does any vehicle (`@takedown_any`).
    pub rams_any: bool,
    /// The player's car is ramming a police car: no wreck from that.
    pub attacking: bool,
    /// A police car is close (EJECT_SAFE): the driver must never be thrown
    /// through the windscreen (wreck.rs), whatever the hit.
    pub police_near: bool,
    /// Share of its speed the car keeps while ramming.
    pub keep: f32,
    /// Hold the car straight while ramming (`@takedown_straight`).
    pub straight: bool,
    /// The car is grinding along a wall or a car (grind.rs; set by the
    /// caller): a hit then is never a crash.
    pub grinding: bool,
    /// A wreck now is a police car's takedown: what to show instead of
    /// WRECKED...
    pub taken_down: Option<&'static str>,
    /// ...and which police car it is (spared by the wreck's clear-up; 0:
    /// none).
    pub taken_by: i32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Shunt,
    Slam,
    TBone,
    Vertical,
    HeadOn,
}

impl Kind {
    /// What Burnout calls the player's hit.
    fn word(self, boosting: bool) -> &'static str {
        match (self, boosting) {
            (Kind::Shunt, false) => "SHUNT",
            (Kind::Shunt, true) => "BOOST SHUNT",
            (Kind::Slam, false) => "SLAM",
            (Kind::Slam, true) => "BOOST SLAM",
            (Kind::TBone, _) => "T-BONE",
            (Kind::Vertical, _) => "VERTICAL",
            (Kind::HeadOn, _) => "HEAD-ON",
        }
    }
}

/// The player's hit on a police car.
#[derive(Clone, Copy)]
struct Hit {
    kind: Kind,
    at: i32,
    /// Vulnerable until (game ms).
    until: i32,
    /// Its speed and heading when hit (for spin-outs).
    speed: f32,
    heading: f32,
}

/// A police car near the player.
#[derive(Default)]
struct Track {
    /// Its velocity last frame.
    vel: Vec3,
    touching: bool,
    last_touch: i32,
    last_air: i32,
    /// Its velocity over the last WINDOW_MS, while not touching the player.
    history: VecDeque<(i32, Vec3)>,
    /// Drivable and on its wheels last frame: only a car that was fine can
    /// crash (one found already wrecked is no one's takedown). Only looked at
    /// close up (CLOSE) or while vulnerable.
    was_ok: bool,
    was_upright: bool,
    police: bool,
}

/// A taken-down car: when, and whether it has blown up yet.
struct Downed {
    at: i32,
    exploded: bool,
}

struct Banner {
    main: &'static str,
    sub: String,
    at: i32,
}

#[derive(Default)]
pub struct Takedowns {
    /// The player's car, where it was and how it was moving last frame,
    /// and when it was last in the air.
    vehicle: i32,
    pos: Vec3,
    vel: Vec3,
    last_air: i32,
    tracks: HashMap<i32, Track>,
    /// Police cars the player hit, while vulnerable.
    hits: HashMap<i32, Hit>,
    downed: HashMap<i32, Downed>,
    /// A police car's hit on the player: which car, how, vulnerable until.
    against: Option<(i32, Kind, i32)>,
    /// The player's car counts as ramming until then (game ms).
    attack_until: i32,
    /// When the player's latest takedowns were (DOUBLE, TRIPLE...).
    recent: VecDeque<i32>,
    banner: Option<Banner>,
    /// The player's car and driver, proofed against a blast until then.
    proofed: Option<(i32, i32, i32)>,
    count: u32,
    /// Police takedowns in this chase.
    chase_count: u32,
    /// The chase was ended by takedowns: the wanted level is kept at zero
    /// until then (the blasts that follow would bring it back).
    escaped_until: i32,
}

/// Police cars this close (m) are watched, other vehicles (`@takedown_any`)
/// this close...
const RANGE: f32 = 40.0;
const TRAFFIC_RANGE: f32 = 25.0;
/// ...and whether they're wrecked looked at this close (m).
const CLOSE: f32 = 15.0;
/// No windscreen ejection with a police car this close (m).
const EJECT_SAFE: f32 = 30.0;
/// Under this closing speed (m/s) a touch is a nudge, not a hit.
const MIN_HIT: f32 = 3.0;
/// The player's car counts as ramming for this long (ms) after a hit; its
/// own speed change is measured over WINDOW_MS from the contact.
const ATTACK_MS: i32 = 400;
/// Speed changes are measured over this (ms, as wreck.rs).
const WINDOW_MS: i32 = 150;
/// A police car hit at SPIN_SPEED (m/s) or more that comes to a stop
/// (under STOPPED) turned SPIN_DEG from where it was heading has spun out.
const SPIN_SPEED: f32 = 12.0;
const STOPPED: f32 = 3.0;
const SPIN_DEG: f32 = 60.0;
/// Rolled over: its up pointing less than ~73 degrees from level.
const ROLLED_UP: f32 = 0.3;
/// Takedowns this close together (ms) are a double, triple...
const MULTI_MS: i32 = 3000;
/// A taken-down car blows up once BLAST_CLEAR m from the player, or after
/// BLAST_MAX ms anyway; never before BLAST_MIN ms (the crash plays first).
const BLAST_CLEAR: f32 = 10.0;
const BLAST_MIN: i32 = 400;
const BLAST_MAX: i32 = 2500;
/// The player's car and driver are proofed against a blast this close (m)
/// for PROOF_MS.
const PROOF_RANGE: f32 = 25.0;
const PROOF_MS: i32 = 2500;
/// Taken-down cars are forgotten this long after (ms).
const FORGET_MS: i32 = 60_000;
const BANNER_MS: i32 = 1800;
/// After a chase ended by takedowns, the wanted level is held at zero this
/// long (ms).
const ESCAPE_HOLD: i32 = 6000;
/// Boost for a hit that isn't (yet) a takedown.
const HIT_BOOST: f32 = 0.03;
const BUSES: [&str; 7] = ["bus", "coach", "airbus", "rentalbus", "tourbus", "pbus", "pbus2"];
/// Police models, for empty police cars (roadblocks).
const POLICE_MODELS: [&str; 13] =
    ["police", "police2", "police3", "police4", "policet", "policeold1", "policeold2", "sheriff", "sheriff2", "fbi", "fbi2", "pranger", "riot"];

fn flat(x: f32, y: f32) -> (f32, f32) {
    let l = x.hypot(y).max(1e-3);
    (x / l, y / l)
}

fn heading_diff(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Not a boat, helicopter, plane or train.
fn is_land(veh: i32) -> bool {
    !matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21)
}

/// A land vehicle driven by the police, or an empty police car.
pub fn is_police(veh: i32) -> bool {
    if !is_land(veh) {
        return false;
    }
    let driver = n::get_ped_in_vehicle_seat(veh, -1);
    if driver == 0 {
        static MODELS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
        let models = MODELS.get_or_init(|| POLICE_MODELS.iter().map(|m| crate::propshield::joaat(m)).collect());
        return models.contains(&n::get_entity_model(veh));
    }
    !n::is_ped_a_player(driver) && matches!(n::get_ped_type(driver), 6 | 27 | 29)
}

/// What a hit by car `a` on car `b` was: their positions, `a`'s velocity
/// and flat forward, whether `a` was just in the air, `b`'s flat forward.
fn classify(a: Vec3, a_vel: Vec3, a_fwd: (f32, f32), a_air: bool, b: Vec3, b_fwd: (f32, f32)) -> Kind {
    if a_air && a_vel.z < -2.0 && a.z - b.z > 0.6 {
        return Kind::Vertical;
    }
    let d = flat(b.x - a.x, b.y - a.y);
    // Where each is on the other: b ahead of a; a ahead of / beside b.
    let ahead_of_a = d.0 * a_fwd.0 + d.1 * a_fwd.1;
    let ahead_of_b = -(d.0 * b_fwd.0 + d.1 * b_fwd.1);
    let beside_b = -(d.0 * b_fwd.1 - d.1 * b_fwd.0);
    let along = a_fwd.0 * b_fwd.0 + a_fwd.1 * b_fwd.1;
    if ahead_of_a > 0.5 && ahead_of_b > 0.5 && along < -0.3 {
        Kind::HeadOn
    } else if ahead_of_a > 0.5 && beside_b.abs() > 0.6 && along.abs() < 0.6 {
        Kind::TBone
    } else if ahead_of_a > 0.5 && ahead_of_b < -0.5 {
        Kind::Shunt
    } else {
        Kind::Slam
    }
}

/// How a police car crashed just now, if it did. `hit`: the player's hit it
/// is vulnerable from; `ok`, `upright`: whether it is drivable and on its
/// wheels now.
fn crashed(s: &TakedownSettings, veh: i32, track: &Track, hit: &Hit, touching: bool, ok: bool, upright: bool) -> Option<&'static str> {
    if track.was_ok && !ok {
        return Some("disabled");
    }
    if track.was_upright && !upright {
        return Some("rolled over");
    }
    // A sudden stop against something (the history only runs while it isn't
    // touching the player's car, so the player's hit itself doesn't count).
    if !touching {
        if let (Some(&(t0, v0)), Some(&(t1, v))) = (track.history.front(), track.history.back()) {
            let dv = (v.x - v0.x).hypot(v.y - v0.y);
            if t1 - t0 >= 30 && dv >= s.crash_dv && v.x.hypot(v.y) <= v0.x.hypot(v0.y) + 1.0 {
                return Some("crashed");
            }
        }
    }
    if hit.speed >= SPIN_SPEED && n::get_entity_speed(veh) < STOPPED && heading_diff(n::get_entity_heading(veh), hit.heading) > SPIN_DEG {
        return Some("spun out");
    }
    None
}

/// What a crashed car hit besides the player's car, as Burnout names a
/// takedown into traffic.
fn struck_traffic(veh: i32, player: i32) -> Option<&'static str> {
    let p = n::get_entity_coords(veh);
    shv::all_vehicles().into_iter().find_map(|o| {
        if o == veh || o == player {
            return None;
        }
        let q = n::get_entity_coords(o);
        if (q.x - p.x).powi(2) + (q.y - p.y).powi(2) > 15.0 * 15.0 || !n::is_entity_touching_entity(veh, o) {
            return None;
        }
        let model = n::get_entity_model(o);
        Some(match n::get_vehicle_class(o) {
            10 | 20 => "TAKEDOWN INTO BIG-RIG!",
            12 => "VAN TAKEDOWN!",
            _ if BUSES.iter().any(|b| crate::propshield::joaat(b) == model) => "BUS TAKEDOWN!",
            _ => "CAR TAKEDOWN!",
        })
    })
}

impl Takedowns {
    /// Police cars chase.rs must leave alone: vulnerable or taken down.
    pub fn hands_off(&self, veh: i32) -> bool {
        self.downed.contains_key(&veh) || self.hits.contains_key(&veh)
    }

    /// Forget the player's side (after a wreck, or out of the car).
    pub fn reset(&mut self) {
        self.vehicle = 0;
        self.tracks.clear();
        self.against = None;
        self.attack_until = 0;
    }

    /// Every frame. `veh`: the car the player is driving (0: none);
    /// `ramming`: police cars going for it (chase.rs).
    pub fn update(&mut self, s: &TakedownSettings, ped: i32, veh: i32, now: i32, boost: &mut Boost, ramming: &dyn Fn(i32) -> bool) -> Context {
        self.blasts(ped, veh, now);
        self.draw(now);
        let player = n::player_id();
        let wanted = n::get_player_wanted_level(player);
        if now < self.escaped_until {
            if wanted > 0 {
                n::clear_player_wanted_level(player);
            }
        } else if wanted == 0 {
            self.chase_count = 0;
        }
        if !s.enabled || veh == 0 {
            self.reset();
            return Context::default();
        }
        let pos = n::get_entity_coords(veh);
        let vel = n::get_entity_velocity(veh);
        if veh != self.vehicle {
            self.reset();
            self.vehicle = veh;
            self.pos = pos;
            self.vel = vel;
        }
        if n::is_entity_in_air(veh) {
            self.last_air = now;
        }
        let f = n::get_entity_forward_vector(veh);
        let fwd = flat(f.x, f.y);
        let window = (s.window * 1000.0) as i32;
        let mut seen = Vec::new();
        let mut police_near = false;
        for other in shv::all_vehicles() {
            if other == veh || self.downed.contains_key(&other) {
                continue;
            }
            let q = n::get_entity_coords(other);
            let d2 = (q.x - pos.x).powi(2) + (q.y - pos.y).powi(2);
            if d2 > RANGE * RANGE || (q.z - pos.z).abs() > 10.0 {
                continue;
            }
            let police = is_police(other);
            if !police && !(s.any && d2 < TRAFFIC_RANGE * TRAFFIC_RANGE && is_land(other)) {
                continue;
            }
            seen.push(other);
            police_near |= police && d2 < EJECT_SAFE * EJECT_SAFE;
            let w = n::get_entity_velocity(other);
            let touching = n::is_entity_touching_entity(veh, other);
            let mut track = self.tracks.remove(&other).unwrap_or_else(|| Track { vel: w, ..Default::default() });
            track.police = police;
            if n::is_entity_in_air(other) {
                track.last_air = now;
            }
            if touching && !track.touching && self.contact(s, other, q, &track, pos, fwd, ramming(other), now, boost) {
                if let Some(h) = self.hits.get(&other).copied() {
                    self.take_down(s, other, h, "smashed", police, veh, now, boost);
                    continue;
                }
            }
            if touching {
                track.last_touch = now;
                track.history.clear();
                // Still pushing it: still vulnerable, still ramming.
                if let Some(h) = self.hits.get_mut(&other) {
                    h.until = h.until.max(now + window);
                    if h.kind != Kind::HeadOn {
                        self.attack_until = now + ATTACK_MS;
                    }
                }
                if let Some(a) = self.against.as_mut().filter(|a| a.0 == other) {
                    a.2 = a.2.max(now + window);
                }
            } else {
                track.history.push_back((now, w));
                while track.history.len() > 2 && now - track.history[1].0 >= WINDOW_MS {
                    track.history.pop_front();
                }
            }
            let look = d2 < CLOSE * CLOSE || self.hits.contains_key(&other);
            let ok = look && n::is_vehicle_driveable(other) && n::get_vehicle_engine_health(other) >= 0.0 && !n::is_entity_on_fire(other);
            let upright = look && n::get_entity_matrix(other).2[2] >= ROLLED_UP;
            if let Some(h) = self.hits.get(&other).copied().filter(|h| now <= h.until) {
                if let Some(how) = crashed(s, other, &track, &h, touching, ok, upright) {
                    self.take_down(s, other, h, how, police, veh, now, boost);
                    continue;
                }
            }
            track.vel = w;
            track.touching = touching;
            track.was_ok = ok;
            track.was_upright = upright;
            self.tracks.insert(other, track);
        }
        self.tracks.retain(|v, _| seen.contains(v));
        self.hits.retain(|_, h| now <= h.until);
        if self.against.is_some_and(|a| now > a.2) {
            self.against = None;
        }
        self.pos = pos;
        self.vel = vel;
        Context {
            rams_police: true,
            rams_any: s.any,
            attacking: now < self.attack_until,
            police_near,
            keep: s.keep,
            straight: s.straight,
            grinding: false,
            taken_down: self.against.map(|a| if a.1 == Kind::Vertical { "VERTICALLY TAKEN DOWN" } else { "TAKEN DOWN" }),
            taken_by: self.against.map_or(0, |a| a.0),
        }
    }

    /// The player's car and police car `other` (at `q`) just touched:
    /// what hit it was, and who hit whom, from how both were moving last
    /// frame. `pos`, `fwd`: the player's car now; `ramming`: the police car
    /// was going for it (a head-on it started is its hit, whoever was
    /// faster). True if the player's hit was hard enough to take it down
    /// there and then.
    #[allow(clippy::too_many_arguments)]
    fn contact(&mut self, s: &TakedownSettings, other: i32, q: Vec3, track: &Track, pos: Vec3, fwd: (f32, f32), ramming: bool, now: i32, boost: &mut Boost) -> bool {
        let d = flat(q.x - pos.x, q.y - pos.y);
        let (v0, w0) = (self.vel, track.vel);
        let mine = v0.x * d.0 + v0.y * d.1;
        let theirs = -(w0.x * d.0 + w0.y * d.1);
        let closing = mine + theirs;
        if closing < MIN_HIT {
            return false;
        }
        let g = n::get_entity_forward_vector(other);
        let their_fwd = flat(g.x, g.y);
        let window = (s.window * 1000.0) as i32;
        let kind = classify(pos, v0, fwd, now - self.last_air < 500, q, their_fwd);
        if mine >= theirs && !(ramming && kind == Kind::HeadOn) {
            let long = matches!(kind, Kind::TBone | Kind::HeadOn);
            let until = now + if long { window * 3 / 2 } else { window };
            self.hits.insert(other, Hit { kind, at: now, until, speed: w0.x.hypot(w0.y), heading: n::get_entity_heading(other) });
            if kind != Kind::HeadOn {
                self.attack_until = now + ATTACK_MS;
            }
            let word = kind.word(boost.is_boosting());
            boost.reward(HIT_BOOST, word, now);
            let what = if track.police { "police car" } else { "vehicle" };
            crate::log(&format!("takedown: {word} on {what} {other}, {:.0} km/h closing", closing * 3.6));
            s.smash > 0.0 && closing >= s.smash
        } else if track.police {
            let kind = classify(q, w0, their_fwd, now - track.last_air < 500, pos, fwd);
            self.against = Some((other, kind, now + window));
            boost.reward(0.0, if kind == Kind::Shunt { "SHUNTED!" } else { "SLAMMED!" }, now);
            crate::log(&format!("takedown: police car {other} hit the player ({kind:?}), {:.0} km/h closing", closing * 3.6));
            false
        } else {
            false // traffic doesn't take the player down
        }
    }

    /// Vehicle `veh` is taken down (`hit`: the player's hit it was
    /// vulnerable from; `police`: a police car).
    #[allow(clippy::too_many_arguments)]
    fn take_down(&mut self, s: &TakedownSettings, veh: i32, hit: Hit, how: &str, police: bool, player: i32, now: i32, boost: &mut Boost) {
        self.hits.remove(&veh);
        self.recent.push_back(now);
        while self.recent.front().is_some_and(|&t| now - t > MULTI_MS) {
            self.recent.pop_front();
        }
        let parked = hit.speed < STOPPED && n::get_ped_in_vehicle_seat(veh, -1) == 0;
        let kind = match hit.kind {
            Kind::Vertical => "VERTICAL TAKEDOWN!",
            Kind::TBone if !parked => "T-BONE TAKEDOWN!",
            _ if parked => "ROADBLOCK TAKEDOWN!",
            _ => struck_traffic(veh, player).unwrap_or("TAKEDOWN!"),
        };
        let multi = match self.recent.len() {
            1 => None,
            2 => Some("DOUBLE TAKEDOWN!"),
            3 => Some("TRIPLE TAKEDOWN!"),
            _ => Some("MULTI TAKEDOWN!"),
        };
        let (mut main, sub) = match multi {
            Some(m) => (m, if kind == "TAKEDOWN!" { "" } else { kind }),
            None => (kind, ""),
        };
        let mut sub = sub.to_string();
        self.count += 1;
        // Enough police taken down: the chase is over.
        if police {
            self.chase_count += 1;
            if s.escape > 0 && self.chase_count >= s.escape {
                main = "YOU ESCAPED!";
                sub = format!("{} POLICE TAKEDOWNS", self.chase_count);
                n::clear_player_wanted_level(n::player_id());
                self.escaped_until = now + ESCAPE_HOLD;
                crate::log(&format!("takedown: chase over after {} police takedowns, wanted level cleared", self.chase_count));
                self.chase_count = 0;
            } else if s.escape > 0 {
                sub = format!("{}{}COPS {} / {}", sub, if sub.is_empty() { "" } else { "   " }, self.chase_count, s.escape);
            }
        }
        boost.reward(s.boost, "", now);
        self.banner = Some(Banner { main, sub, at: now });
        n::play_sound_frontend("MEDAL_UP", "HUD_MINI_GAME_SOUNDSET");
        // Finished: its driver stops driving it, and chase.rs's toughness
        // goes, so it can burn.
        n::set_entity_invincible(veh, false);
        n::set_vehicle_strong(veh, false);
        n::set_disable_vehicle_fires(veh, false);
        n::set_vehicle_out_of_control(veh, false, false);
        if s.launch > 0.0 {
            self.launch(veh, player, s.launch);
        }
        self.downed.insert(veh, Downed { at: now, exploded: !s.explode });
        let what = if police { "police car" } else { "vehicle" };
        let sub = self.banner.as_ref().map_or("", |b| b.sub.as_str());
        crate::log(&format!("TAKEDOWN #{}: {main} {sub} {what} {veh} ({how}, {} ms after a {:?})", self.count, now - hit.at, hit.kind));
    }

    /// Throws a taken-down car off the player's path: out to the side it's
    /// on, a little ahead, up and tumbling, so it's clear when it blows up.
    fn launch(&self, veh: i32, player: i32, speed: f32) {
        let q = n::get_entity_coords(veh);
        let p = n::get_entity_coords(player);
        let (dx, dy) = (q.x - p.x, q.y - p.y);
        // Along the player's path (or straight away from it, standing still).
        let f = if self.vel.x.hypot(self.vel.y) > 2.0 { flat(self.vel.x, self.vel.y) } else { flat(dx, dy) };
        let side = if dx * f.1 - dy * f.0 >= 0.0 { 1.0 } else { -1.0 };
        let out = (f.1 * side, -f.0 * side);
        let w = n::get_entity_velocity(veh);
        n::set_entity_velocity(veh, w.x + (out.0 + f.0 * 0.3) * speed, w.y + (out.1 + f.1 * 0.3) * speed, w.z.max(0.0) + speed * 0.5);
        // Rolling away from the player, about its own length.
        n::set_entity_angular_velocity(veh, f.0 * 3.0 * side, f.1 * 3.0 * side, 1.5 * side);
    }

    /// Taken-down cars blow up once clear of the player (or after a while
    /// anyway); the player's car and driver are proofed against the blast
    /// for a moment.
    fn blasts(&mut self, ped: i32, veh: i32, now: i32) {
        if let Some((car, driver, until)) = self.proofed {
            if now > until {
                if car != 0 && n::does_entity_exist(car) {
                    n::set_entity_proofs(car, false, false, false);
                }
                n::set_entity_invincible(driver, false);
                self.proofed = None;
            }
        }
        if self.downed.is_empty() {
            return;
        }
        let p = n::get_entity_coords(if veh != 0 { veh } else { ped });
        let mut blasts = Vec::new();
        self.downed.retain(|&car, d| {
            if !n::does_entity_exist(car) || now - d.at > FORGET_MS {
                return false;
            }
            if !d.exploded {
                let q = n::get_entity_coords(car);
                let dist = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (q.z - p.z).powi(2)).sqrt();
                let age = now - d.at;
                if age >= BLAST_MIN && (dist >= BLAST_CLEAR || age >= BLAST_MAX) {
                    d.exploded = true;
                    blasts.push((car, dist));
                }
            }
            true
        });
        for (car, dist) in blasts {
            if dist < PROOF_RANGE {
                if veh != 0 {
                    n::set_entity_proofs(veh, true, true, false);
                }
                n::set_entity_invincible(ped, true);
                self.proofed = Some((veh, ped, now + PROOF_MS));
            }
            n::explode_vehicle(car);
            crate::log(&format!("takedown: police car {car} blown up, {dist:.0} m from the player"));
        }
    }

    /// The takedown banner: pops in big, then fades.
    fn draw(&mut self, now: i32) {
        let Some(b) = &self.banner else { return };
        let t = now - b.at;
        if !(0..=BANNER_MS).contains(&t) {
            self.banner = None;
            return;
        }
        let pop = 1.0 + 0.35 * (1.0 - t as f32 / 150.0).max(0.0);
        let fade = ((BANNER_MS - t) as f32 / 300.0).min(1.0);
        let a = (255.0 * fade) as i32;
        n::draw_text_centred(b.main, 0.5, 0.25, 1.25 * pop, [255, 190, 20, a]);
        if !b.sub.is_empty() {
            n::draw_text_centred(&b.sub, 0.5, 0.335, 0.7, [255, 255, 255, a]);
        }
    }
}
