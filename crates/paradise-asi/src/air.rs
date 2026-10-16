//! Burnout-style flight for the player's car (`@air_control`, `@barrel_roll`).
//!
//! GTA lets a car tumble however it left the ground. Burnout keeps it
//! steady: while the player's car is in the air this steers its rotation
//! towards a calm attitude, nose along the way it's flying (pitch following
//! the arc only half way, so landings stay flat), wheels down. `@air_control`
//! is how firmly (0 = GTA as it was, 1 = held steady).
//!
//! Barrel rolls: leaving a split ramp (one side higher) sends the car off
//! rolling, either way, and it keeps rolling; a straight jump only jolts it
//! for a moment. So over the first ROLL_WINDOW of a jump the roll is left
//! alone and averaged: at `@barrel_roll_trigger` degrees per second or more
//! (with enough air ahead) the roll is taken over and timed to turn exactly
//! once before it lands, wheels down, Burnout style. Take-offs that rolled
//! but didn't make it are logged with the reason. Touching down for less
//! than LANDING (bouncing off the lip of a ramp) doesn't end a jump.
//!
//! The car's spin is measured from how its orientation changes between
//! frames (world space), and set with SET_ENTITY_ANGULAR_VELOCITY (world
//! space).

use crate::natives as n;
use std::f32::consts::TAU;

#[derive(Clone, Copy)]
pub struct AirSettings {
    /// How firmly the car is held steady in the air (0 = off, 1 = held).
    pub control: f32,
    pub barrel_roll: bool,
    /// Average roll rate early in a jump (degrees per second) that starts a
    /// barrel roll: the higher, the harder a ramp must roll the car.
    pub barrel_trigger: f32,
}

impl Default for AirSettings {
    fn default() -> Self {
        Self { control: 0.7, barrel_roll: true, barrel_trigger: 90.0 }
    }
}

type V = [f32; 3];
/// An orientation: right, forward and up (world).
pub type Frame = (V, V, V);

/// World spin (rad/s) from an orientation `dt` s ago to now.
pub fn world_spin(was: Frame, now: Frame, dt: f32) -> V {
    if dt <= 1e-4 {
        return [0.0; 3];
    }
    scale(add(add(cross(was.0, now.0), cross(was.1, now.1)), cross(was.2, now.2)), 0.5 / dt)
}

fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V, k: f32) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn norm(a: V) -> V {
    let l = dot(a, a).sqrt().max(1e-6);
    scale(a, 1.0 / l)
}

/// Signed roll (rad, around the nose `f`) that would bring the car's up `u`
/// level.
fn roll_to_level(f: V, u: V) -> f32 {
    let level = norm(add([0.0, 0.0, 1.0], scale(f, -f[2])));
    dot(f, cross(u, level)).atan2(dot(u, level))
}

/// Air time (game s) before the attitude control starts, so small hops
/// over bumps are left alone.
const SETTLE: f32 = 0.12;
/// How fast the attitude is corrected (1/s) at full control...
const GAIN: f32 = 4.0;
/// ...and the fastest it turns the car to do it (rad/s).
const MAX_TURN: f32 = 5.0;
/// Share of the flight path's climb or dive the nose follows.
const PITCH_FOLLOW: f32 = 0.5;
/// Largest nose up / down it aims for (radians, 25 degrees).
const MAX_PITCH: f32 = 0.44;
/// The roll is averaged over this much of a jump (s) to decide on a barrel
/// roll...
const ROLL_WINDOW: f32 = 0.2;
/// ...which needs at least this much air time still ahead (s)...
const ROLL_MIN_AIR: f32 = 0.45;
/// ...at this speed (m/s) or more.
const ROLL_MIN_SPEED: f32 = 12.0;
/// The roll is timed to finish this share of the way to the landing.
const ROLL_FINISH: f32 = 0.75;
/// Roll rate limits during a barrel roll (rad/s).
const ROLL_MIN_RATE: f32 = 3.0;
const ROLL_MAX_RATE: f32 = 14.0;
/// On the ground this long (s), a jump has ended.
const LANDING: f32 = 0.15;

/// A barrel roll in progress.
struct BarrelRoll {
    /// +1 or -1: which way it turns.
    dir: f32,
    /// How far it has turned (rad, in `dir`).
    done: f32,
}

#[derive(Default)]
pub struct Air {
    vehicle: i32,
    /// Orientation last frame.
    last: Option<Frame>,
    /// Game time in the air so far (s).
    airborne: f32,
    roll: Option<BarrelRoll>,
    /// How far it has rolled so far in this jump (rad).
    rolled: f32,
    /// Game time back on the ground (s).
    grounded: f32,
    /// This jump's barrel roll chance has been decided.
    decided: bool,
}

impl Air {
    /// Every frame, for the player's car (0: not driving). `gravity` is the
    /// airborne gravity, times normal (`@air_gravity`).
    pub fn update(&mut self, veh: i32, s: &AirSettings, gravity: f32) {
        if veh == 0 || veh != self.vehicle {
            *self = Self { vehicle: veh, ..Default::default() };
            if veh == 0 {
                return;
            }
        }
        let (r, f, u) = n::get_entity_matrix(veh);
        let dt = n::get_frame_time().clamp(0.0, 0.1);
        // World spin from the change in orientation since last frame.
        let w = self.last.replace((r, f, u)).map_or([0.0; 3], |was| world_spin(was, (r, f, u), dt));
        let roll_rate = dot(w, f);
        if !n::is_entity_in_air(veh) || matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) {
            self.grounded += dt;
            if let Some(roll) = self.roll.as_mut() {
                roll.done += roll_rate * roll.dir * dt;
            }
            if self.airborne > 0.0 && self.grounded < LANDING {
                return; // a bounce, the jump goes on
            }
            if let Some(roll) = self.roll.take() {
                crate::log(&format!("barrel roll: landed after {:.0} degrees", roll.done.to_degrees()));
            }
            self.airborne = 0.0;
            self.rolled = 0.0;
            self.decided = false;
            return;
        }
        self.grounded = 0.0;
        self.airborne += dt;
        let v = n::get_entity_velocity(veh);
        let vel = [v.x, v.y, v.z];
        let speed = dot(vel, vel).sqrt();
        let g = 9.81 * gravity.max(0.1);
        // Air time left: falling from this height above the ground, at this
        // climb rate.
        let h = n::get_entity_height_above_ground(veh).max(0.0);
        let air_left = (v.z + (v.z * v.z + 2.0 * g * h).sqrt()) / g;

        if let Some(roll) = self.roll.as_mut() {
            roll.done += roll_rate * roll.dir * dt;
        } else if s.barrel_roll && !self.decided {
            self.rolled += roll_rate * dt;
        }
        if s.barrel_roll && !self.decided && self.roll.is_none() && self.airborne >= ROLL_WINDOW {
            // How fast it kept rolling, on average, since take-off.
            let rate = self.rolled / self.airborne;
            let trigger = s.barrel_trigger.to_radians();
            self.decided = true;
            if rate.abs() >= trigger && speed >= ROLL_MIN_SPEED && air_left >= ROLL_MIN_AIR {
                crate::log(&format!("barrel roll: {:.0} deg/s off the ramp, {:.2} s of air ahead", rate.to_degrees(), air_left));
                // Counting from level: it has already rolled part of the
                // way, and must end wheels down.
                let dir = rate.signum();
                self.roll = Some(BarrelRoll { dir, done: -roll_to_level(f, u) * dir });
            } else if rate.abs() >= trigger * 0.5 {
                let why = if rate.abs() < trigger {
                    "rolling too slowly"
                } else if speed < ROLL_MIN_SPEED {
                    "too slow"
                } else {
                    "too little air"
                };
                crate::log(&format!(
                    "barrel roll: not started, {why} ({:.0} deg/s, {:.0} km/h, {:.2} s of air ahead)",
                    rate.to_degrees(),
                    speed * 3.6,
                    air_left
                ));
            }
        }

        let k = s.control.clamp(0.0, 1.0);
        let rolling = self.roll.as_ref().is_some_and(|b| b.done < TAU - 0.15);
        if (k <= 0.0 || self.airborne < SETTLE) && !rolling {
            return;
        }

        // Where the nose should point: the way it's flying, climbing or
        // diving only part of the way.
        let horiz = [v.x, v.y, 0.0];
        let ahead = if dot(horiz, horiz) > 25.0 {
            norm(horiz)
        } else {
            norm([f[0], f[1], 0.0])
        };
        let climb = (v.z.atan2(dot(horiz, horiz).sqrt()) * PITCH_FOLLOW).clamp(-MAX_PITCH, MAX_PITCH);
        let want_f = add(scale(ahead, climb.cos()), [0.0, 0.0, climb.sin()]);
        // Turn the nose towards it (pitch and yaw)...
        let mut want_w = scale(cross(f, want_f), GAIN);
        // ...and roll: level, or round once for a barrel roll.
        let spin = match &self.roll {
            Some(b) if rolling => {
                let left = TAU - b.done;
                let time = (air_left * ROLL_FINISH).max(0.05);
                b.dir * (left / time).clamp(ROLL_MIN_RATE, ROLL_MAX_RATE)
            }
            // Still deciding on a barrel roll: let it roll.
            _ if s.barrel_roll && !self.decided => roll_rate,
            _ => {
                roll_to_level(f, u) * GAIN
            }
        };
        want_w = add(want_w, scale(f, spin));
        let mag = dot(want_w, want_w).sqrt();
        let limit = if rolling { ROLL_MAX_RATE + MAX_TURN } else { MAX_TURN };
        if mag > limit {
            want_w = scale(want_w, limit / mag);
        }
        // Blend the spin it has towards the one wanted; a barrel roll is
        // taken over fully.
        let blend = if rolling { 1.0 } else { 1.0 - (-10.0 * k * dt).exp() };
        let new_w = add(w, scale(add(want_w, scale(w, -1.0)), blend));
        n::set_entity_angular_velocity(veh, new_w[0], new_w[1], new_w[2]);
    }
}
