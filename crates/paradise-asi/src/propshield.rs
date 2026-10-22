//! Street props (lamp posts, traffic lights, bins, signs...) never hurt a
//! car, Burnout style.
//!
//! The props placed for Burnout's are listed in `paradise_props.csv` (game
//! folder, written by `bpx gta-props`). While the player's car is about to
//! reach one, it takes no crash damage and no visible damage; an impact on
//! the side facing a prop is a prop being knocked over, not a crash (see
//! wreck.rs, which also gives back the speed it cost and throws the prop
//! along with the car, so heavy ones like benches go flying too).
//!
//! Props never hurt the car at all: while shielded the car also keeps the
//! engine, body and fuel tank health it had when the shield went up, and
//! any fire is put out (GTA still lit fuel tanks after bench hits). Vehicles always do: the shield drops
//! just before another vehicle makes contact (one closing in on the car, so
//! GTA's own crash damage applies; traffic cruising alongside doesn't
//! count), and an impact while touching another vehicle is always a crash.
//!
//! Going through props is only a visual effect: they never change the
//! car's motion. Props still collide with it (so GTA knocks standing ones
//! over, and pieces bounce off it), but the moment a prop starts touching
//! the car, the car's speed, direction and spin go back to what they were
//! the frame before. A standing prop the car hits is then let pass through
//! it for GHOST_MS while it's thrown clear, so it can't drag along with the
//! car. (Not while another vehicle is touching: that's a crash.)
//!
//! The gameplay camera ignores props the car has just hit or thrown, so it
//! doesn't pull in close as they pass through and past the car.
//!
//! Every frame, loose objects (thrown props and broken pieces) in the space
//! just ahead of the moving car's nose, along the way it's moving, are swept
//! aside, away from its line (ones dead ahead go up and over), so nothing
//! gets pushed along unseen and slips under the car. Heavy props (lamp posts
//! and benches, by model: GTA doesn't tell scripts an object's mass) are
//! thrown well clear: hard to the side, or high over the car and off to one
//! side, so they don't land back in its path.

use crate::natives as n;
use crate::shv::Vec3;
use std::collections::HashMap;

const FILE: &str = "paradise_props.csv";
const CELL: f32 = 32.0;
/// Props within this (m), plus how far the car goes in LOOKAHEAD s, shield it.
const NEAR: f32 = 5.0;
const LOOKAHEAD: f32 = 0.35;
/// A struck side within this (m) of a prop counts as a prop hit.
const HIT: f32 = 5.0;

#[derive(Default)]
pub struct PropShield {
    /// Position and model hash of each prop.
    cells: HashMap<(i32, i32), Vec<([f32; 3], u32)>>,
    /// The vehicle currently shielded.
    shielded: i32,
    /// Its engine, body and fuel tank health when the shield went up.
    guard: [f32; 3],
    /// Objects thrown recently, and when.
    thrown: HashMap<i32, i32>,
    /// Loose objects (thrown props, broken pieces) near the car this frame:
    /// position and model.
    loose: Vec<([f32; 3], u32)>,
    /// Models of the heavy props, which get the strong sweep.
    heavy: std::collections::HashSet<u32>,
    /// Models of all the props.
    models: std::collections::HashSet<u32>,
    /// Props knocked loose (hit or thrown): they no longer stand.
    loose_props: std::collections::HashSet<i32>,
    /// Props passing through the car until then (game ms).
    ghost: HashMap<i32, i32>,
    /// Props touching the car last frame.
    touching: std::collections::HashSet<i32>,
    /// When the car last hit a prop (game ms).
    last_hit: i32,
    /// The car, its velocity, spin and orientation last frame.
    last: Option<(i32, Vec3, [f32; 3], crate::air::Frame)>,
}

/// How long (ms) a standing prop the car hits passes through it.
const GHOST_MS: i32 = 500;
/// A prop moving slower than this (m/s) that was never knocked loose is
/// still standing.
const STANDING_SPEED: f32 = 0.5;

/// Prop models (name contains one of these) that get the strong sweep.
const HEAVY: [&str; 2] = ["streetlight", "bench"];

/// The sweep zone ahead of the car's centre, along the way it moves (m):
/// ahead from..to, half width, and height either way.
const SWEEP_FROM: f32 = 1.5;
const SWEEP_TO: f32 = 5.0;
/// The zone reaches further ahead by this much (m) per m/s of speed.
const SWEEP_TO_PER_SPEED: f32 = 0.08;
const SWEEP_HALF_WIDTH: f32 = 1.8;
const SWEEP_HEIGHT: f32 = 1.5;
/// Only while the car moves forward faster than this (m/s).
const SWEEP_SPEED: f32 = 4.0;
/// Objects closer than this (m) to the car's centre line go up and over.
const SWEEP_CENTRE: f32 = 0.35;

fn cell(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (y / CELL).floor() as i32)
}

impl PropShield {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Ok(text) = std::fs::read_to_string(FILE) else {
            crate::log("prop shield: paradise_props.csv not found");
            return s;
        };
        let mut n = 0;
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            let v: Vec<f32> = f.iter().take(3).filter_map(|x| x.parse().ok()).collect();
            if v.len() == 3 {
                let model = f.get(3).map(|m| joaat(m)).unwrap_or(0);
                if f.get(3).is_some_and(|m| HEAVY.iter().any(|h| m.to_ascii_lowercase().contains(h))) {
                    s.heavy.insert(model);
                }
                if model != 0 {
                    s.models.insert(model);
                }
                s.cells.entry(cell(v[0], v[1])).or_default().push(([v[0], v[1], v[2]], model));
                n += 1;
            }
        }
        crate::log(&format!("prop shield: {n} props, {} heavy models", s.heavy.len()));
        s
    }

    /// Props within `r` of `p` (same height give or take 5 m).
    fn near(&self, p: Vec3, r: f32) -> impl Iterator<Item = &([f32; 3], u32)> {
        let (cx, cy) = cell(p.x, p.y);
        let reach = (r / CELL).ceil() as i32;
        (cx - reach..=cx + reach)
            .flat_map(move |x| (cy - reach..=cy + reach).map(move |y| (x, y)))
            .filter_map(|c| self.cells.get(&c))
            .flatten()
            .filter(move |(q, _)| (q[0] - p.x).powi(2) + (q[1] - p.y).powi(2) <= r * r && (q[2] - p.z).abs() < 5.0)
    }

    /// Every frame: shield the player's car (`veh`, 0 if none) while a prop
    /// is about to be reached, where it was placed or (thrown, broken)
    /// wherever it is now. Call after `sweep`.
    pub fn update(&mut self, veh: i32) {
        let want = veh != 0 && {
            let p = n::get_entity_coords(veh);
            let r = NEAR + n::get_entity_speed(veh) * LOOKAHEAD;
            !self.loose.is_empty() || self.near(p, r).next().is_some()
        };
        if want && vehicle_contact(veh) {
            self.release();
            return;
        }
        if want && self.shielded != veh {
            self.release();
            n::set_vehicle_strong(veh, true);
            n::set_vehicle_can_be_visibly_damaged(veh, false);
            self.shielded = veh;
            self.guard = [n::get_vehicle_engine_health(veh), n::get_vehicle_body_health(veh), n::get_vehicle_petrol_tank_health(veh)];
        } else if !want {
            self.release();
        }
        if self.shielded != 0 {
            let v = self.shielded;
            let [engine, body, tank] = self.guard;
            if n::get_vehicle_engine_health(v) < engine {
                n::set_vehicle_engine_health(v, engine);
            }
            if n::get_vehicle_body_health(v) < body {
                n::set_vehicle_body_health(v, body);
            }
            if n::get_vehicle_petrol_tank_health(v) < tank {
                n::set_vehicle_petrol_tank_health(v, tank);
            }
            if n::is_entity_on_fire(v) {
                n::stop_entity_fire(v);
            }
        }
    }

    /// Is the car protected from damage right now?
    /// When the car last hit a prop (game ms; 0: never).
    pub fn last_hit(&self) -> i32 {
        self.last_hit
    }

    pub fn shielding(&self) -> bool {
        self.shielded != 0
    }

    fn release(&mut self) {
        if self.shielded != 0 {
            n::set_vehicle_strong(self.shielded, false);
            n::set_vehicle_can_be_visibly_damaged(self.shielded, true);
            self.shielded = 0;
        }
    }

    /// Was an impact on the side of the car facing `dir` (world x / y, unit)
    /// a prop being knocked over? Gives the props on that side.
    /// Nearest first.
    pub fn struck_props(&self, veh: i32, dir: (f32, f32)) -> Vec<([f32; 3], u32)> {
        let p = n::get_entity_coords(veh);
        let dist = |q: &[f32; 3]| (q[0] - p.x).hypot(q[1] - p.y);
        let mut hit: Vec<([f32; 3], u32)> = self
            .near(p, HIT)
            .chain(self.loose.iter().filter(|(q, _)| (q[0] - p.x).hypot(q[1] - p.y) <= HIT))
            .filter(|(q, _)| {
                let (dx, dy) = (q[0] - p.x, q[1] - p.y);
                let d = dx.hypot(dy).max(0.1);
                (dx * dir.0 + dy * dir.1) / d > 0.3
            })
            .copied()
            .collect();
        hit.sort_by(|a, b| dist(&a.0).total_cmp(&dist(&b.0)));
        hit
    }
}

impl PropShield {
    /// Every frame: notes loose objects near the car (for the shield), keeps
    /// props from changing the car's motion, and throws loose objects about
    /// to go under its nose.
    pub fn sweep(&mut self, veh: i32, now: i32) {
        self.thrown.retain(|_, t| now - *t < 600);
        self.ghost.retain(|_, t| *t > now);
        for &o in self.ghost.keys().chain(self.thrown.keys()) {
            n::camera_ignore(o);
        }
        self.loose.clear();
        if veh == 0 {
            self.last = None;
            self.touching.clear();
            return;
        }
        let reach = NEAR + n::get_entity_speed(veh) * LOOKAHEAD;
        let p = n::get_entity_coords(veh);
        let v = n::get_entity_velocity(veh);
        let frame = n::get_entity_matrix(veh);
        let w = match self.last {
            Some((was, _, _, f)) if was == veh => crate::air::world_spin(f, frame, n::get_frame_time()),
            _ => [0.0; 3],
        };
        // The car's motion before anything touched it this frame.
        let before = self.last.filter(|l| l.0 == veh).map(|l| (l.1, l.2));
        let speed = v.x.hypot(v.y);
        let sweeping = speed >= SWEEP_SPEED;
        // Along the way the car moves (sliding sideways too).
        let (fx, fy) = (v.x / speed.max(1e-3), v.y / speed.max(1e-3));
        let sweep_to = SWEEP_TO + speed * SWEEP_TO_PER_SPEED;
        let objects = crate::shv::all_objects();
        let mut touching = std::collections::HashSet::new();
        let mut struck = false;
        for &o in &objects {
            let q = n::get_entity_coords(o);
            let (dx, dy, dz) = (q.x - p.x, q.y - p.y, q.z - p.z);
            let d2 = dx * dx + dy * dy;
            if d2 > reach * reach || dz.abs() >= 5.0 {
                continue;
            }
            let model = n::get_entity_model(o);
            self.loose.push(([q.x, q.y, q.z], model));
            let is_prop = self.models.contains(&model) || self.loose_props.contains(&o);
            if self.ghost.contains_key(&o) {
                n::set_entity_no_collision_entity(veh, o, true);
                n::set_entity_no_collision_entity(o, veh, true);
            } else if is_prop
                && d2 < 8.0 * 8.0
                // Touching, or (a fast hit GTA may not report as touching) a
                // standing prop suddenly flying right next to the car.
                && (n::is_entity_touching_entity(veh, o) || (!self.loose_props.contains(&o) && d2 < 6.0 * 6.0 && n::get_entity_speed(o) > 2.0))
            {
                touching.insert(o);
                if !self.touching.contains(&o) {
                    struck = true;
                    self.last_hit = now;
                    if !self.loose_props.contains(&o) {
                        // A standing prop hit: knocked over, through the car,
                        // thrown clear along the way the car was going.
                        self.loose_props.insert(o);
                        self.ghost.insert(o, now + GHOST_MS);
                        n::set_entity_no_collision_entity(veh, o, true);
                        n::set_entity_no_collision_entity(o, veh, true);
                        let bv = before.map_or(v, |b| b.0);
                        let l = bv.x.hypot(bv.y).max(1e-3);
                        let (bx, by) = (bv.x / l, bv.y / l);
                        throw(o, self.heavy.contains(&model), dx * by - dy * bx, bv, (bx, by));
                        self.thrown.insert(o, now);
                    }
                }
            }
            if !sweeping || d2 > sweep_to * sweep_to + SWEEP_HALF_WIDTH * SWEEP_HALF_WIDTH || dz.abs() > SWEEP_HEIGHT {
                continue;
            }
            if self.thrown.contains_key(&o) {
                continue;
            }
            // Standing props stay put until the car hits them.
            if self.models.contains(&model) && !self.loose_props.contains(&o) && n::get_entity_speed(o) < STANDING_SPEED {
                continue;
            }
            let ahead = dx * fx + dy * fy;
            let side = dx * fy - dy * fx;
            if ahead < SWEEP_FROM || ahead > sweep_to || side.abs() > SWEEP_HALF_WIDTH {
                continue;
            }
            throw(o, self.heavy.contains(&model), side, v, (fx, fy));
            self.thrown.insert(o, now);
            if self.models.contains(&model) {
                self.loose_props.insert(o);
            }
        }
        self.touching = touching;
        // Forget objects that are gone.
        if self.loose_props.len() > 64 {
            let alive: std::collections::HashSet<i32> = objects.iter().copied().collect();
            self.loose_props.retain(|o| alive.contains(o));
        }
        // A prop's touch never changes the car's motion (a vehicle's does).
        let (mut v, mut w) = (v, w);
        if struck && !touching_vehicle(veh) {
            if let Some((bv, bw)) = before {
                n::set_entity_velocity(veh, bv.x, bv.y, bv.z);
                n::set_entity_angular_velocity(veh, bw[0], bw[1], bw[2]);
                (v, w) = (bv, bw);
            }
        }
        self.last = Some((veh, v, w, frame));
    }

    /// A prop the car just knocked over (it stood fixed until hit, so the
    /// sweep couldn't move it): thrown clear the same way.
    pub fn toss(&mut self, veh: i32, props: &[([f32; 3], u32)], v: Vec3, now: i32) {
        self.last_hit = now;
        let p = n::get_entity_coords(veh);
        let (fx, fy) = {
            let l = v.x.hypot(v.y).max(1e-3);
            (v.x / l, v.y / l)
        };
        for &(q, model) in props {
            if model == 0 {
                continue;
            }
            let o = n::get_closest_object_of_type(Vec3::new(q[0], q[1], q[2] + 0.5), 3.0, model);
            if o != 0 {
                let (dx, dy) = (q[0] - p.x, q[1] - p.y);
                throw(o, self.heavy.contains(&model), dx * fy - dy * fx, v, (fx, fy));
                self.thrown.insert(o, now);
            }
        }
    }
}

/// Throws object `o` out of the way of a car moving at `v` along (fx, fy);
/// `side` is how far (m) right of the car's centre line it is. Light props
/// are knocked aside (or, dead ahead, up over the bonnet); heavy ones (lamp
/// posts, benches) are thrown well clear: hard to the side, or high over
/// the car and off to one side so they come down out of its path.
fn throw(o: i32, heavy: bool, side: f32, v: Vec3, (fx, fy): (f32, f32)) {
    let speed = v.x.hypot(v.y);
    // Right of travel is (fy, -fx); dead-centre ones go right.
    let sign = if side < 0.0 { -1.0 } else { 1.0 };
    let centre = side.abs() < SWEEP_CENTRE;
    let (out, up) = match (heavy, centre) {
        (false, true) => (0.0, 2.0 + 0.06 * speed),
        (false, false) => (sign * (2.0 + 0.25 * speed), 0.5 + 0.02 * speed),
        (true, true) => (sign * (3.0 + 0.25 * speed), 6.0 + 0.2 * speed),
        (true, false) => (sign * (6.0 + 0.7 * speed), 2.0 + 0.08 * speed),
    };
    let along = if !heavy && centre { 1.05 } else { 1.0 };
    n::set_entity_velocity(o, v.x * along + fy * out, v.y * along - fx * out, v.z + up);
}

/// Another vehicle about to hit the car (centres this close, plus the
/// closing speed over CONTACT_LEAD s) drops the shield.
const CONTACT_GAP: f32 = 3.0;
const CONTACT_LEAD: f32 = 0.25;
/// Only vehicles closing in faster than this (m/s) count.
const CONTACT_CLOSING: f32 = 2.0;

/// Is another vehicle touching the car, or about to hit it?
fn vehicle_contact(veh: i32) -> bool {
    let p = n::get_entity_coords(veh);
    let v = n::get_entity_velocity(veh);
    crate::shv::all_vehicles().into_iter().any(|o| {
        if o == veh {
            return false;
        }
        let q = n::get_entity_coords(o);
        let (dx, dy, dz) = (q.x - p.x, q.y - p.y, q.z - p.z);
        let d = dx.hypot(dy);
        if d > 25.0 || dz.abs() > 4.0 {
            return false;
        }
        let w = n::get_entity_velocity(o);
        // Closing speed along the line between them.
        let closing = ((v.x - w.x) * dx + (v.y - w.y) * dy) / d.max(0.1);
        (closing > CONTACT_CLOSING && d < CONTACT_GAP + closing * CONTACT_LEAD) || (d < 8.0 && n::is_entity_touching_entity(veh, o))
    })
}

/// Is the car touching another vehicle (a car-on-car impact)?
pub fn touching_vehicle(veh: i32) -> bool {
    let p = n::get_entity_coords(veh);
    crate::shv::all_vehicles().into_iter().any(|o| {
        if o == veh {
            return false;
        }
        let q = n::get_entity_coords(o);
        (q.x - p.x).powi(2) + (q.y - p.y).powi(2) < 12.0 * 12.0 && n::is_entity_touching_entity(veh, o)
    })
}

/// Rockstar's case-insensitive one-at-a-time hash (model names).
pub(crate) fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_add(c as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}
