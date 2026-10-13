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
//! Every frame, loose objects (props and broken pieces of them) in the space
//! just ahead of the moving car's nose are swept aside, away from the car's
//! centre line (ones dead ahead go up and over), so nothing gets pushed
//! along unseen and slips under the car. Heavy props (lamp posts and
//! benches, by model: GTA doesn't tell scripts an object's mass) are thrown
//! well clear: hard to the side, or high over the car and off to one side,
//! so they don't land back in its path.

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
}

/// Prop models (name contains one of these) that get the strong sweep.
const HEAVY: [&str; 2] = ["streetlight", "bench"];

/// The sweep zone ahead of the car's centre, in car space (m): forward
/// from..to, half width, and height either way.
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
    /// Every frame: notes loose objects near the car (for the shield) and
    /// throws those about to go under the car's nose.
    pub fn sweep(&mut self, veh: i32, now: i32) {
        self.thrown.retain(|_, t| now - *t < 600);
        self.loose.clear();
        if veh == 0 {
            return;
        }
        let local_v = n::get_entity_speed_vector_relative(veh);
        let sweeping = local_v.y >= SWEEP_SPEED;
        let reach = NEAR + n::get_entity_speed(veh) * LOOKAHEAD;
        let p = n::get_entity_coords(veh);
        let f = n::get_entity_forward_vector(veh);
        let (fx, fy) = {
            let l = f.x.hypot(f.y).max(1e-3);
            (f.x / l, f.y / l)
        };
        let v = n::get_entity_velocity(veh);
        let speed = v.x.hypot(v.y);
        let sweep_to = SWEEP_TO + speed * SWEEP_TO_PER_SPEED;
        for o in crate::shv::all_objects() {
            let q = n::get_entity_coords(o);
            let (dx, dy, dz) = (q.x - p.x, q.y - p.y, q.z - p.z);
            let d2 = dx * dx + dy * dy;
            if d2 <= reach * reach && dz.abs() < 5.0 {
                self.loose.push(([q.x, q.y, q.z], n::get_entity_model(o)));
            }
            if !sweeping || d2 > sweep_to * sweep_to + SWEEP_HALF_WIDTH * SWEEP_HALF_WIDTH || dz.abs() > SWEEP_HEIGHT {
                continue;
            }
            if self.thrown.contains_key(&o) {
                continue;
            }
            let ahead = dx * fx + dy * fy;
            let side = dx * fy - dy * fx;
            if ahead < SWEEP_FROM || ahead > sweep_to || side.abs() > SWEEP_HALF_WIDTH {
                continue;
            }
            throw(o, self.heavy.contains(&n::get_entity_model(o)), side, v, (fx, fy));
            self.thrown.insert(o, now);
        }
    }

    /// A prop the car just knocked over (it stood fixed until hit, so the
    /// sweep couldn't move it): thrown clear the same way.
    pub fn toss(&mut self, veh: i32, props: &[([f32; 3], u32)], v: Vec3, now: i32) {
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
fn joaat(s: &str) -> u32 {
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
