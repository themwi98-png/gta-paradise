//! Traffic at junctions, from `paradise_junctions.txt` (bpx gta-junctions:
//! Burnout's traffic lights, junctions found where lanes cross, and ones
//! marked in the viewer's Junctions panel).
//!
//! GTA's traffic knows nothing about Paradise City's junctions (our roads
//! have no GTA junction data), so cars from every side drive in at once and
//! lock up. Here:
//!
//! - Burnout's traffic lights run their own cycles (each state's time, times
//!   `@light_time`), switch the GTA traffic light props standing in for them
//!   (or draw Burnout's lamps where there's no prop, `@light_lamps`), and
//!   hold GTA's cars at red: the first car brakes to a stop at the stop
//!   line, the cars behind stop in a queue, and on green they go again, front
//!   first. A car too close to stop when its light changes carries on.
//! - At junctions without lights, and on light approaches that have no stop
//!   line, a car about to drive in gives way while a car crossing its path is
//!   moving inside the junction (`@give_way`).
//! - A car stuck inside a junction for `@unjam` seconds, out of the player's
//!   sight, is removed: a jam that can't clear itself.
//!
//! Cars are stopped with BRING_VEHICLE_TO_HALT (`@light_hold halt`), which
//! pauses the driver's own driving, or by capping their top speed
//! (`@light_hold cap`). Only GTA's own traffic is held: not the player, police
//! with sirens on, or parked and mission vehicles.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::{HashMap, HashSet};

const FILE: &str = "paradise_junctions.txt";

/// `@junctions`, `@light_time`, ... settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct JunctionSettings {
    pub enabled: bool,
    /// Burnout's light state times are multiplied by this.
    pub light_time: f32,
    pub lamps: bool,
    pub give_way: bool,
    /// Seconds a car stuck in a junction (out of sight) waits before it's removed; 0 never.
    pub unjam: f32,
    /// Stop cars by capping their speed instead of the halt native.
    pub cap: bool,
}

impl Default for JunctionSettings {
    fn default() -> Self {
        Self { enabled: true, light_time: 1.0, lamps: true, give_way: true, unjam: 10.0, cap: false }
    }
}

/// Junctions this close (m) to the player run.
const ACTIVE: f32 = 260.0;
/// Cars this close (m) to the player are controlled.
const CONTROL: f32 = 200.0;
/// Light props are looked for this close, every PROP_SCAN_MS.
const PROP_RANGE: f32 = 220.0;
const PROP_SCAN_MS: i32 = 1000;
/// Lamps are drawn this close.
const LAMP_RANGE: f32 = 160.0;
/// A light that just turned from green shows amber this long (s).
const AMBER: f32 = 3.0;
/// How far back from a stop line (m) cars count as waiting at it.
const QUEUE: f32 = 70.0;
/// The first car stops this far (m) before the line; each car behind this
/// much further back.
const STOP_GAP: f32 = 1.0;
const SPACING: f32 = 8.0;
/// A car is held once stopping at its place needs this much braking (m/s²)...
const HOLD_DECEL: f32 = 2.2;
/// ...and carries on through a light that just changed when stopping would
/// need more than this.
const MAX_DECEL: f32 = 5.0;
/// "Just changed": red for less than this (s).
const JUST_RED: f32 = 1.5;
/// Cars held at a light go again this long (ms) after the car in front.
const STAGGER_MS: i32 = 600;
/// A car is matched to a stop line within its lane's half width plus this (m).
const LANE_SLACK: f32 = 1.5;
/// Heading (cos) a car must have along a lane to be in it.
const ALIGNED: f32 = 0.7;
/// A held car that isn't matched to its stop line for this long is let go.
const LOST_MS: i32 = 1500;
/// The halt native is given again at most this often (ms), and holds this
/// long (s) unless let go sooner.
const REISSUE_MS: i32 = 300;
const HALT_SECS: i32 = 10;
/// Give way: cars this far (m) outside a junction, heading in, look before entering...
const LOOK: f32 = 22.0;
/// ...for cars inside it moving at least this fast (m/s) across their path
/// (headings further apart than this, cos)...
const MOVING: f32 = 1.0;
const CROSSING: f32 = 0.82;
/// ...and wait at most this long (ms) before going anyway.
const MAX_GIVE_WAY_MS: i32 = 15_000;
/// Clear for this long (ms) before a car that gave way goes.
const CLEAR_MS: i32 = 600;
/// Stuck: slower than this (m/s) inside a junction.
const STILL: f32 = 0.6;
/// Stuck cars this close (m) to the player, or in view within UNJAM_SEEN, stay.
const UNJAM_NEAR: f32 = 25.0;
const UNJAM_SEEN: f32 = 90.0;
/// Vehicle facts (population type, class) are looked up again after this (ms).
const FACTS_MS: i32 = 3000;
const REPORT_MS: i32 = 60_000;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Signal {
    Green,
    Amber,
    Red,
}

struct Stop {
    set: usize,
    pos: Vec3,
    dir: (f32, f32),
    half_width: f32,
    /// The stop line, then back along the lane.
    back: Vec<Vec3>,
}

struct PropLight {
    set: usize,
    model: u32,
    pos: Vec3,
    entity: i32,
    shown: i32,
}

struct Lamp {
    set: usize,
    colour: u8,
    pos: Vec3,
}

struct Junction {
    id: String,
    lit: bool,
    centre: Vec3,
    radius: f32,
    /// (seconds, stopped light sets).
    states: Vec<(f32, u8)>,
    /// Where in its cycle this junction starts (s): not every light in town changes at once.
    phase: f32,
    stops: Vec<Stop>,
    props: Vec<PropLight>,
    lamps: Vec<Lamp>,
}

impl Junction {
    /// Each light set's signal at time `t` (s), and how long the current state has run.
    fn signals(&self, t: f32, scale: f32) -> [Signal; 8] {
        let mut out = [Signal::Green; 8];
        if self.states.is_empty() {
            return out;
        }
        let cycle: f32 = self.states.iter().map(|s| s.0 * scale).sum::<f32>().max(0.1);
        let mut x = (t + self.phase).rem_euclid(cycle);
        let mut i = 0;
        while i + 1 < self.states.len() && x >= self.states[i].0 * scale {
            x -= self.states[i].0 * scale;
            i += 1;
        }
        let (mask, prev) = (self.states[i].1, self.states[(i + self.states.len() - 1) % self.states.len()].1);
        for (k, s) in out.iter_mut().enumerate() {
            let bit = 1u8 << k;
            *s = if mask & bit == 0 {
                Signal::Green
            } else if prev & bit == 0 && x < AMBER {
                Signal::Amber
            } else {
                Signal::Red
            };
        }
        out
    }

    /// Red for less than JUST_RED s (the light only just changed)?
    fn just_red(&self, t: f32, scale: f32, set: usize) -> bool {
        self.signals(t - JUST_RED, scale)[set] != Signal::Red
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Why {
    Light(usize, usize),
    GiveWay(usize),
}

struct Held {
    why: Why,
    since: i32,
    seen: i32,
    issued: i32,
    /// When its light turned green (it goes STAGGER_MS per car ahead later).
    green: Option<i32>,
    rank: usize,
    /// Give way: since when the way has been clear.
    clear: Option<i32>,
}

/// A car near a junction this frame.
#[derive(Clone, Copy)]
struct Car {
    v: i32,
    pos: Vec3,
    vel: Vec3,
    /// Unit heading on the ground.
    fwd: (f32, f32),
    speed: f32,
}

#[derive(Default)]
struct Counts {
    held: u32,
    ran: u32,
    gave_way: u32,
    unjammed: u32,
    props: u32,
}

pub struct Junctions {
    list: Vec<Junction>,
    held: HashMap<i32, Held>,
    /// Junction a car went into on green (or too close to stop): not held there.
    committed: HashMap<i32, (usize, i32)>,
    /// Since when a car has been stuck inside a junction.
    still: HashMap<i32, i32>,
    /// (can be held, an emergency vehicle, checked at).
    facts: HashMap<i32, (bool, bool, i32)>,
    last_props: i32,
    last_report: i32,
    counts: Counts,
    was_enabled: bool,
}

fn joaat(s: &str) -> u32 {
    crate::propshield::joaat(s)
}

fn flat(x: f32, y: f32) -> (f32, f32) {
    let l = (x * x + y * y).sqrt();
    if l < 1e-4 {
        (0.0, 0.0)
    } else {
        (x / l, y / l)
    }
}

fn d2(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn parse(text: &str) -> Vec<Junction> {
    let mut out: Vec<Junction> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| f.get(i).and_then(|s| s.parse::<f32>().ok());
        match f.first().copied() {
            Some("J") => {
                let (Some(x), Some(y), Some(z), Some(r)) = (num(3), num(4), num(5), num(6)) else { continue };
                let id = f[1].to_string();
                let phase = (joaat(&id) % 1000) as f32 * 0.1;
                out.push(Junction { lit: f[2] == "L", id, centre: Vec3::new(x, y, z), radius: r, states: Vec::new(), phase, stops: Vec::new(), props: Vec::new(), lamps: Vec::new() });
            }
            Some("S") => {
                let (Some(j), Some(t), Some(m)) = (out.last_mut(), num(1), num(2)) else { continue };
                j.states.push((t.max(0.5), m as u8));
            }
            Some("T") => {
                let Some(j) = out.last_mut() else { continue };
                let (Some(set), Some(x), Some(y), Some(z), Some(dx), Some(dy), Some(w)) = (num(1), num(2), num(3), num(4), num(5), num(6), num(7)) else { continue };
                let pos = Vec3::new(x, y, z);
                let mut back = vec![pos];
                let rest: Vec<f32> = f[8..].iter().filter_map(|s| s.parse().ok()).collect();
                back.extend(rest.as_chunks::<3>().0.iter().map(|c| Vec3::new(c[0], c[1], c[2])));
                j.stops.push(Stop { set: (set as usize).min(7), pos, dir: flat(dx, dy), half_width: w, back });
            }
            Some("G") => {
                let Some(j) = out.last_mut() else { continue };
                let (Some(set), Some(model), Some(x), Some(y), Some(z)) = (num(1), f.get(2), num(3), num(4), num(5)) else { continue };
                j.props.push(PropLight { set: (set as usize).min(7), model: joaat(model), pos: Vec3::new(x, y, z), entity: 0, shown: -1 });
            }
            Some("C") => {
                let Some(j) = out.last_mut() else { continue };
                let (Some(set), Some(colour), Some(x), Some(y), Some(z)) = (num(1), num(2), num(3), num(4), num(5)) else { continue };
                j.lamps.push(Lamp { set: (set as usize).min(7), colour: colour as u8, pos: Vec3::new(x, y, z) });
            }
            _ => {}
        }
    }
    out
}

/// Where a car is along a stop line's lane: (metres before the line, metres
/// off the lane centre), if it's on the lane and heading along it.
fn on_lane(stop: &Stop, c: &Car) -> Option<(f32, f32)> {
    // Just past the line still counts (it's crossing it).
    let past = (c.pos.x - stop.pos.x) * stop.dir.0 + (c.pos.y - stop.pos.y) * stop.dir.1;
    let side = ((c.pos.x - stop.pos.x) * stop.dir.1 - (c.pos.y - stop.pos.y) * stop.dir.0).abs();
    if (0.0..3.0).contains(&past) && side <= stop.half_width + LANE_SLACK && c.fwd.0 * stop.dir.0 + c.fwd.1 * stop.dir.1 >= ALIGNED {
        return Some((-past, side));
    }
    let mut run = 0.0;
    let mut best: Option<(f32, f32)> = None;
    for w in stop.back.windows(2) {
        // Segment from w[1] (further back) towards w[0] (nearer the line).
        let (a, b) = (w[1], w[0]);
        let (sx, sy) = (b.x - a.x, b.y - a.y);
        let len = (sx * sx + sy * sy).sqrt().max(1e-3);
        let (ux, uy) = (sx / len, sy / len);
        let t = ((c.pos.x - a.x) * ux + (c.pos.y - a.y) * uy).clamp(0.0, len);
        let (px, py) = (a.x + ux * t, a.y + uy * t);
        let off = ((c.pos.x - px).powi(2) + (c.pos.y - py).powi(2)).sqrt();
        let z = a.z + (b.z - a.z) * t / len;
        if off <= stop.half_width + LANE_SLACK && (c.pos.z - z).abs() < 4.0 && c.fwd.0 * ux + c.fwd.1 * uy >= ALIGNED && best.is_none_or(|b| off < b.1) {
            best = Some((run + (len - t), off));
        }
        run += len;
        if run > QUEUE {
            break;
        }
    }
    best
}

impl Junctions {
    pub fn load() -> Self {
        let list = std::fs::read_to_string(FILE).map(|t| parse(&t)).unwrap_or_default();
        let lit = list.iter().filter(|j| j.lit).count();
        let stops: usize = list.iter().map(|j| j.stops.len()).sum();
        let props: usize = list.iter().map(|j| j.props.len()).sum();
        let lamps: usize = list.iter().map(|j| j.lamps.len()).sum();
        crate::log(&format!(
            "junctions: {} ({lit} with traffic lights: {stops} stop lines, {props} light props, {lamps} lamps; {} without){}",
            list.len(),
            list.len() - lit,
            if list.is_empty() { " - no paradise_junctions.txt" } else { "" }
        ));
        Self {
            list,
            held: HashMap::new(),
            committed: HashMap::new(),
            still: HashMap::new(),
            facts: HashMap::new(),
            last_props: 0,
            last_report: 0,
            counts: Counts::default(),
            was_enabled: false,
        }
    }

    fn release(v: i32, s: &JunctionSettings) {
        if !n::does_entity_exist(v) {
            return;
        }
        if s.cap {
            n::set_vehicle_max_speed(v, 0.0);
        } else {
            n::stop_bringing_vehicle_to_halt(v);
        }
    }

    /// Lets every held car go and gives the light props back to GTA.
    fn stop_all(&mut self, s: &JunctionSettings) {
        for &v in self.held.keys() {
            Self::release(v, s);
        }
        self.held.clear();
        for j in &mut self.list {
            for p in &mut j.props {
                if p.entity != 0 && n::does_entity_exist(p.entity) {
                    n::set_entity_trafficlight_override(p.entity, 3);
                }
                p.entity = 0;
                p.shown = -1;
            }
        }
    }

    /// Can this car be held (GTA traffic, not parked, mission or a boat),
    /// and is it an emergency vehicle?
    fn facts(&mut self, v: i32, now: i32) -> (bool, bool) {
        if let Some(&(ok, emergency, at)) = self.facts.get(&v) {
            if now - at < FACTS_MS {
                return (ok, emergency);
            }
        }
        let class = n::get_vehicle_class(v);
        let ok = matches!(n::get_entity_population_type(v), 1 | 3 | 4 | 5) && !matches!(class, 14 | 15 | 16 | 21) && !n::is_entity_dead(v);
        self.facts.insert(v, (ok, class == 18, now));
        (ok, class == 18)
    }

    /// Holds a car `dist` m from where it should stop.
    fn hold(&mut self, c: &Car, dist: f32, why: Why, rank: usize, s: &JunctionSettings, now: i32) {
        let dist = dist.max(0.3);
        let h = self.held.entry(c.v).or_insert_with(|| Held { why, since: now, seen: now, issued: i32::MIN / 2, green: None, rank, clear: None });
        h.why = why;
        h.seen = now;
        h.rank = rank;
        if s.cap {
            n::set_vehicle_max_speed(c.v, (2.0 * HOLD_DECEL * (dist - 0.3).max(0.0)).sqrt().max(0.02));
            n::set_vehicle_brake_lights(c.v, true);
        } else if now - h.issued >= REISSUE_MS && !n::is_vehicle_being_brought_to_halt(c.v) {
            n::bring_vehicle_to_halt(c.v, dist, HALT_SECS);
            h.issued = now;
        }
    }

    pub fn update(&mut self, s: &JunctionSettings, player_ped: i32, player_pos: Vec3, now: i32) {
        if !s.enabled || self.list.is_empty() {
            if self.was_enabled {
                self.stop_all(s);
                self.was_enabled = false;
            }
            return;
        }
        self.was_enabled = true;
        let t = now as f32 / 1000.0;
        let scale = s.light_time;
        let active: Vec<usize> = (0..self.list.len()).filter(|&i| d2(self.list[i].centre, player_pos) < ACTIVE + self.list[i].radius).collect();
        let signals: HashMap<usize, [Signal; 8]> = active.iter().map(|&i| (i, self.list[i].signals(t, scale))).collect();

        // The lights themselves.
        let scan = now - self.last_props >= PROP_SCAN_MS;
        if scan {
            self.last_props = now;
        }
        for &i in &active {
            let near = d2(self.list[i].centre, player_pos);
            let sig = signals[&i];
            let j = &mut self.list[i];
            if near < PROP_RANGE {
                for p in &mut j.props {
                    if scan && (p.entity == 0 || !n::does_entity_exist(p.entity)) {
                        p.entity = n::get_closest_object_of_type(p.pos, 1.5, p.model);
                        p.shown = -1;
                        if p.entity != 0 {
                            self.counts.props += 1;
                        }
                    }
                    let want = match sig[p.set] {
                        Signal::Green => 0,
                        Signal::Red => 1,
                        Signal::Amber => 2,
                    };
                    if p.entity != 0 && p.shown != want {
                        n::set_entity_trafficlight_override(p.entity, want);
                        p.shown = want;
                    }
                }
            }
            if s.lamps && near < LAMP_RANGE {
                for l in &j.lamps {
                    let on = matches!((sig[l.set], l.colour), (Signal::Red, 0) | (Signal::Amber, 1) | (Signal::Green, 2));
                    if on {
                        let rgb = match l.colour {
                            0 => [255, 30, 20],
                            1 => [255, 160, 0],
                            _ => [30, 255, 80],
                        };
                        n::draw_marker(28, l.pos, 0.18, [rgb[0], rgb[1], rgb[2], 230]);
                        n::draw_light_with_range(l.pos, rgb, 3.0, 2.0);
                    }
                }
            }
        }

        // The cars near the junctions in play.
        let mine = if n::is_ped_in_any_vehicle(player_ped) { n::get_vehicle_ped_is_in(player_ped) } else { 0 };
        let mut cars: Vec<(Car, Vec<usize>)> = Vec::new();
        for v in shv::all_vehicles() {
            if v == mine {
                continue;
            }
            let pos = n::get_entity_coords(v);
            if d2(pos, player_pos) > CONTROL {
                continue;
            }
            let near: Vec<usize> = active.iter().copied().filter(|&i| d2(self.list[i].centre, pos) < self.list[i].radius + QUEUE && (self.list[i].centre.z - pos.z).abs() < 15.0).collect();
            if near.is_empty() {
                continue;
            }
            let (ok, emergency) = self.facts(v, now);
            if !ok || (emergency && n::is_vehicle_siren_on(v)) {
                continue;
            }
            let vel = n::get_entity_velocity(v);
            let f = n::get_entity_forward_vector(v);
            cars.push((Car { v, pos, vel, fwd: flat(f.x, f.y), speed: (vel.x * vel.x + vel.y * vel.y).sqrt() }, near));
        }

        // Lights: each stop line's queue, nearest the line first.
        let mut at_red: HashSet<i32> = HashSet::new();
        let mut queues: HashMap<(usize, usize), Vec<(usize, f32)>> = HashMap::new();
        let mut matched = vec![false; cars.len()];
        for (ci, (c, near)) in cars.iter().enumerate() {
            let mut best: Option<((usize, usize), f32, f32)> = None;
            for &ji in near {
                for (si, stop) in self.list[ji].stops.iter().enumerate() {
                    if let Some((along, off)) = on_lane(stop, c) {
                        if best.is_none_or(|b| off < b.2) {
                            best = Some(((ji, si), along, off));
                        }
                    }
                }
            }
            if let Some((key, along, _)) = best {
                queues.entry(key).or_default().push((ci, along));
                matched[ci] = true;
            }
        }
        for ((ji, si), mut queue) in queues {
            queue.sort_by(|a, b| a.1.total_cmp(&b.1));
            let set = self.list[ji].stops[si].set;
            let dir = self.list[ji].stops[si].dir;
            let sig = signals[&ji][set];
            // Places in the queue go to the cars that will stop.
            let mut stopping = 0usize;
            for &(ci, along) in &queue {
                let c = cars[ci].0;
                if sig == Signal::Green {
                    self.committed.insert(c.v, (ji, now));
                    if let Some(h) = self.held.get_mut(&c.v) {
                        h.seen = now;
                        h.green.get_or_insert(now);
                    }
                    continue;
                }
                if self.committed.get(&c.v).is_some_and(|&(cj, _)| cj == ji) {
                    self.committed.insert(c.v, (ji, now));
                    continue;
                }
                at_red.insert(c.v);
                let toward = (c.vel.x * dir.0 + c.vel.y * dir.1).max(0.0);
                let to_go = along - (STOP_GAP + stopping as f32 * SPACING);
                if !self.held.contains_key(&c.v) {
                    let need = toward * toward / (2.0 * to_go.max(0.1));
                    // Too close to stop when the light changed: carry on.
                    if stopping == 0 && ((along < STOP_GAP + 0.5 && toward > 3.0) || (need > MAX_DECEL && (sig == Signal::Amber || self.list[ji].just_red(t, scale, set)))) {
                        self.committed.insert(c.v, (ji, now));
                        self.counts.ran += 1;
                        continue;
                    }
                    if need < HOLD_DECEL && to_go > 6.0 {
                        stopping += 1;
                        continue; // not yet
                    }
                    self.counts.held += 1;
                }
                if let Some(h) = self.held.get_mut(&c.v) {
                    h.green = None;
                }
                // Already level with the car in front (side by side): stop where it is.
                self.hold(&c, if to_go < -1.0 { 0.5 } else { to_go }, Why::Light(ji, si), stopping, s, now);
                stopping += 1;
            }
        }

        // Give way where there's no light for the car.
        if s.give_way {
            for (ci, (c, near)) in cars.iter().enumerate() {
                if matched[ci] {
                    continue;
                }
                let mut wait: Option<(usize, f32)> = None;
                for &ji in near {
                    let j = &self.list[ji];
                    let out = d2(c.pos, j.centre) - j.radius;
                    if !(0.0..LOOK).contains(&out) {
                        continue;
                    }
                    let to = flat(j.centre.x - c.pos.x, j.centre.y - c.pos.y);
                    if c.fwd.0 * to.0 + c.fwd.1 * to.1 < 0.5 {
                        continue;
                    }
                    // At a lit junction, only approaches without a stop line.
                    if j.lit && j.stops.iter().any(|st| c.fwd.0 * st.dir.0 + c.fwd.1 * st.dir.1 > 0.8) {
                        continue;
                    }
                    let crossing = cars.iter().any(|(o, _)| {
                        o.v != c.v
                            && o.speed > MOVING
                            && d2(o.pos, j.centre) < j.radius
                            && (o.pos.z - j.centre.z).abs() < 8.0
                            && ((o.vel.x * c.fwd.0 + o.vel.y * c.fwd.1) / o.speed).abs() < CROSSING
                    });
                    if crossing {
                        wait = Some((ji, out));
                        break;
                    }
                }
                let car = *c;
                let held = self.held.get(&c.v).map(|h| (matches!(h.why, Why::GiveWay(_)), h.since));
                match (wait, held) {
                    (Some((ji, out)), Some((true, since))) => {
                        if let Some(h) = self.held.get_mut(&c.v) {
                            h.clear = None;
                        }
                        if now - since < MAX_GIVE_WAY_MS {
                            self.hold(&car, out - 1.0, Why::GiveWay(ji), 0, s, now);
                        }
                    }
                    (Some((ji, out)), None) => {
                        let need = c.speed * c.speed / (2.0 * (out - 1.0).max(0.1));
                        if need <= MAX_DECEL {
                            self.counts.gave_way += 1;
                            self.hold(&car, out - 1.0, Why::GiveWay(ji), 0, s, now);
                        }
                    }
                    (None, Some((true, _))) => {
                        if let Some(h) = self.held.get_mut(&c.v) {
                            h.seen = now;
                            h.clear.get_or_insert(now);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Let held cars go: green (front first), the way clear, or lost.
        let mut go: Vec<i32> = Vec::new();
        for (&v, h) in &self.held {
            let gone = !n::does_entity_exist(v) || now - h.seen > LOST_MS;
            let free = match h.why {
                Why::Light(..) => h.green.is_some_and(|g| now - g >= h.rank as i32 * STAGGER_MS),
                Why::GiveWay(_) => h.clear.is_some_and(|c| now - c >= CLEAR_MS) || now - h.since >= MAX_GIVE_WAY_MS,
            };
            if gone || free {
                go.push(v);
            }
        }
        for v in go {
            if let Some(h) = self.held.remove(&v) {
                if let Why::Light(ji, _) = h.why {
                    self.committed.insert(v, (ji, now));
                }
            }
            Self::release(v, s);
        }
        self.committed.retain(|_, &mut (_, at)| now - at < 4000);

        // Jams: stuck inside a junction, out of the player's sight.
        if s.unjam > 0.0 {
            let mut stuck_now: HashMap<i32, i32> = HashMap::new();
            for (c, near) in &cars {
                let Some(inside) = near.iter().copied().find(|&ji| d2(c.pos, self.list[ji].centre) < self.list[ji].radius) else { continue };
                if c.speed >= STILL || self.held.contains_key(&c.v) || at_red.contains(&c.v) {
                    continue;
                }
                let since = *self.still.get(&c.v).unwrap_or(&now);
                stuck_now.insert(c.v, since);
                let far = d2(c.pos, player_pos);
                if (now - since) as f32 >= s.unjam * 1000.0 && far > UNJAM_NEAR && (far > UNJAM_SEEN || !n::is_entity_on_screen(c.v)) {
                    for seat in -1..8 {
                        let p = n::get_ped_in_vehicle_seat(c.v, seat);
                        if p != 0 && !n::is_ped_a_player(p) {
                            n::delete_ped(p);
                        }
                    }
                    n::delete_vehicle(c.v);
                    stuck_now.remove(&c.v);
                    self.counts.unjammed += 1;
                    crate::log(&format!("junctions: removed a car stuck {:.0} s in junction {} at ({:.1}, {:.1}, {:.1})", (now - since) as f32 / 1000.0, self.list[inside].id, c.pos.x, c.pos.y, c.pos.z));
                }
            }
            self.still = stuck_now;
        }
        self.facts.retain(|_, &mut (_, _, at)| now - at < FACTS_MS * 4);

        if now - self.last_report >= REPORT_MS {
            if self.last_report != 0 {
                let c = &self.counts;
                crate::log(&format!(
                    "junctions: {} in play, {} cars held now; last minute: {} held at red, {} carried on through a changing light, {} gave way, {} stuck cars removed, {} light props found",
                    active.len(),
                    self.held.len(),
                    c.held,
                    c.ran,
                    c.gave_way,
                    c.unjammed,
                    c.props
                ));
            }
            self.counts = Counts::default();
            self.last_report = now;
        }
    }
}
