//! Burnout-style grinding (`@grind`): scraping along a wall, a barrier or
//! another car at speed barely slows the player's car and barely damages
//! it, and throws sparks.
//!
//! Each frame, rays go out sideways from the car's front, middle and back
//! on both sides (map, vehicles and objects). A hit within a few cm of the
//! car's side, on a surface the car is moving along (not into: under 30
//! degrees off its direction of travel), is a grind contact. While
//! grinding (contacts less than GAP_MS apart):
//! - the car keeps its speed, losing only `@grind_drag` of it a second (it
//!   still slows when braking, and it's never pushed faster than it was),
//!   sliding along the surface instead of into it (not while ramming a
//!   police car: its slowing down stays);
//! - it takes only `@grind_damage` of the damage (and no dents);
//! - sparks fly from the contact point (GTA's sparks for a rim on the road
//!   after a tyre bursts, `rim_fric_hard` at full friction, kept on the
//!   contact point; and metal-on-metal bangs against cars), `@grind_sparks`
//!   times their size;
//! - a hit there is never a crash (wreck.rs: Event::Grind).

use crate::natives as n;
use crate::shv::Vec3;

#[derive(Clone, Copy)]
pub struct GrindSettings {
    pub enabled: bool,
    /// Slowest speed (m/s) that grinds.
    pub min_speed: f32,
    /// Share of its speed the car loses each second of grinding.
    pub drag: f32,
    /// Share of the damage the car still takes.
    pub damage: f32,
    /// Size of the sparks (0: none).
    pub sparks: f32,
}

impl Default for GrindSettings {
    fn default() -> Self {
        Self { enabled: true, min_speed: 12.0, drag: 0.05, damage: 0.1, sparks: 1.0 }
    }
}

/// How far out from the car's side (m) the rays look, and how close a hit
/// must be to be touching.
const REACH: f32 = 0.35;
const TOUCH: f32 = 0.2;
/// The surface may face up or down this much (normal z) and still be a wall.
const WALL_Z: f32 = 0.6;
/// Moving into the surface at more than this (sine of the angle) is a hit,
/// not a grind.
const GLANCING: f32 = 0.5;
/// Contacts this close together (ms) are one grind.
const GAP_MS: i32 = 150;
const BANG_MS: i32 = 180;
const BRAKE: i32 = 72;
const PTFX: &str = "core";
/// GTA's burst-tyre rim sparks (wheelfx.dat's friction effect for a rim on
/// any hard surface), and its evolution for how hard it rubs.
const SPARKS: &str = "rim_fric_hard";
const SPARKS_EVO: &str = "friction";

struct Contact {
    at: Vec3,
    /// The surface's normal, flat, pointing out towards the car.
    normal: (f32, f32),
    vehicle: bool,
}

#[derive(Default)]
pub struct Grind {
    vehicle: i32,
    /// When the last contact was (game ms).
    last_contact: i32,
    active: bool,
    since: i32,
    /// The speed the car is kept at (m/s).
    held: f32,
    /// Engine and body health when the grind started.
    health: (f32, f32),
    /// The looped sparks effect (0: none).
    fx: i32,
    last_bang: i32,
    /// The car's size (model bounding box), for where to look.
    dims: (Vec3, Vec3),
    on_car: bool,
}

impl Grind {
    /// One frame for the player's car `veh` (0: not driving). `shielding`:
    /// the prop shield has the car's dents switched off itself. `ramming`:
    /// it's ramming a police car, whose slowing down (takedown.rs) stays.
    /// Returns whether the car is grinding.
    pub fn update(&mut self, s: &GrindSettings, veh: i32, shielding: bool, ramming: bool, now: i32) -> bool {
        if veh != self.vehicle {
            self.end(shielding, now);
            self.vehicle = veh;
            if veh != 0 {
                self.dims = n::model_dimensions(n::get_entity_model(veh));
            }
        }
        if veh == 0 || !s.enabled {
            self.end(shielding, now);
            return false;
        }
        let v = n::get_entity_velocity(veh);
        let speed = v.x.hypot(v.y);
        let contact = if speed >= s.min_speed && !n::is_entity_in_air(veh) { self.contact(veh, v, speed) } else { None };
        if contact.is_some() {
            self.last_contact = now;
        }
        if !self.active {
            let Some(c) = &contact else { return false };
            self.active = true;
            self.since = now;
            self.held = speed;
            self.on_car = c.vehicle;
            self.health = (n::get_vehicle_engine_health(veh), n::get_vehicle_body_health(veh));
            if !shielding {
                n::set_vehicle_can_be_visibly_damaged(veh, false);
            }
            crate::log(&format!("grind: started against {} at {:.1} m/s", if c.vehicle { "a car" } else { "a wall" }, speed));
        } else if now - self.last_contact > GAP_MS {
            self.end(shielding, now);
            return false;
        }
        // Speed: kept, less the drag; braking or going faster resets it.
        let dt = n::get_frame_time();
        self.held *= 1.0 - s.drag.clamp(0.0, 1.0) * dt;
        if speed > self.held || n::get_disabled_control_normal(BRAKE).max(n::get_control_normal(BRAKE)) > 0.3 {
            self.held = speed;
        }
        if let Some(c) = &contact {
            self.on_car |= c.vehicle;
            if ramming {
                self.held = speed;
            } else if speed < self.held - 0.2 && speed > 0.1 {
                // Along the surface: what's left of the motion once any of
                // it into the surface is taken out.
                let into = v.x * c.normal.0 + v.y * c.normal.1;
                let (mut dx, mut dy) = (v.x, v.y);
                if into < 0.0 {
                    dx -= c.normal.0 * into;
                    dy -= c.normal.1 * into;
                }
                let len = dx.hypot(dy).max(0.01);
                n::set_entity_velocity(veh, dx / len * self.held, dy / len * self.held, v.z);
            }
            if s.sparks > 0.0 {
                self.sparks(veh, s.sparks, c, v, now);
            }
        }
        // Damage: only `@grind_damage` of what it has cost since it started.
        let keep = |was: f32, now: f32| was - (was - now) * s.damage.clamp(0.0, 1.0);
        let (engine, body) = (n::get_vehicle_engine_health(veh), n::get_vehicle_body_health(veh));
        if engine < self.health.0 {
            n::set_vehicle_engine_health(veh, keep(self.health.0, engine));
        }
        if body < self.health.1 {
            n::set_vehicle_body_health(veh, keep(self.health.1, body));
        }
        true
    }

    fn end(&mut self, shielding: bool, now: i32) {
        if !self.active {
            return;
        }
        self.active = false;
        self.stop_sparks();
        if self.vehicle != 0 && n::does_entity_exist(self.vehicle) && !shielding {
            n::set_vehicle_can_be_visibly_damaged(self.vehicle, true);
        }
        crate::log(&format!("grind: ended after {:.1} s (against {})", (now - self.since) as f32 / 1000.0, if self.on_car { "a car" } else { "walls" }));
    }

    /// The closest grind contact along either side of the car, if any.
    fn contact(&self, veh: i32, v: Vec3, speed: f32) -> Option<Contact> {
        let (r, f, u) = n::get_entity_matrix(veh);
        let pos = n::get_entity_coords(veh);
        let (lo, hi) = self.dims;
        let half_width = hi.x.max(-lo.x);
        let up = (lo.z + hi.z) * 0.5;
        let (vx, vy) = (v.x / speed, v.y / speed);
        let mut best: Option<(f32, Contact)> = None;
        for side in [-1.0f32, 1.0] {
            for along in [lo.y * 0.65, 0.0, hi.y * 0.65] {
                let from = Vec3::new(pos.x + f[0] * along + u[0] * up, pos.y + f[1] * along + u[1] * up, pos.z + f[2] * along + u[2] * up);
                let out = side * (half_width + REACH);
                let to = Vec3::new(from.x + r[0] * out, from.y + r[1] * out, from.z + r[2] * out);
                let Some(h) = n::raycast(from, to, 1 | 2 | 16, veh) else { continue };
                let gap = ((h.pos.x - from.x).powi(2) + (h.pos.y - from.y).powi(2) + (h.pos.z - from.z).powi(2)).sqrt() - half_width;
                let flat = h.normal.x.hypot(h.normal.y);
                if gap > TOUCH || h.normal.z.abs() > WALL_Z || flat < 0.1 {
                    continue;
                }
                let normal = (h.normal.x / flat, h.normal.y / flat);
                if (vx * normal.0 + vy * normal.1).abs() > GLANCING {
                    continue;
                }
                if best.as_ref().is_none_or(|(g, _)| gap < *g) {
                    let vehicle = h.entity != 0 && n::is_entity_a_vehicle(h.entity);
                    best = Some((gap, Contact { at: h.pos, normal, vehicle }));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    fn stop_sparks(&mut self) {
        if self.fx != 0 {
            n::stop_ptfx(self.fx);
            self.fx = 0;
        }
    }

    /// The sparks on the contact point: GTA's rim sparks, started (looped,
    /// on the car) at the first contact and moved to each new one.
    fn sparks(&mut self, veh: i32, scale: f32, c: &Contact, v: Vec3, now: i32) {
        if !n::request_ptfx_asset(PTFX) {
            return;
        }
        let (r, f, u) = n::get_entity_matrix(veh);
        let p = n::get_entity_coords(veh);
        let rel = [c.at.x - p.x, c.at.y - p.y, c.at.z - p.z];
        let dot = |a: [f32; 3]| a[0] * rel[0] + a[1] * rel[1] + a[2] * rel[2];
        let at = Vec3::new(dot(r), dot(f), dot(u));
        if self.fx == 0 || !n::does_ptfx_exist(self.fx) {
            self.fx = n::ptfx_looped_on_entity(PTFX, SPARKS, veh, at, 1.5 * scale);
            if self.fx == 0 {
                crate::log("grind: the sparks effect didn't start");
            }
        } else {
            n::set_ptfx_offset(self.fx, at);
        }
        if self.fx != 0 {
            n::set_ptfx_evolution(self.fx, SPARKS_EVO, 1.0);
        }
        // Facing back along the car's travel: the bang's bits trail behind.
        let back = (-v.x).atan2(v.y).to_degrees() + 180.0;
        if c.vehicle && now - self.last_bang >= BANG_MS {
            self.last_bang = now;
            n::ptfx_at(PTFX, "bang_carmetal", c.at, back, 0.8 * scale);
        }
    }
}
