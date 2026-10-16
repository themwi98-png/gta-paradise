//! Burnout-style drift assist for the player's car (`@drift_assist`).
//!
//! GTA simulates a slide straight: once the rear steps out the car keeps
//! rotating, and catching it makes the grip snap back and whip the car the
//! other way. Burnout quietly holds a drift at a sensible angle instead.
//! While the car slides on the ground at speed, this:
//! - stops the drift angle (between where the car points and where it's
//!   going) growing past `@drift_angle`, easing the rotation back;
//! - damps the rotation as the car comes back straight, so a caught slide
//!   doesn't flick into a spin the other way;
//! - steadies the slide in between, a little.
//! Normal driving (no slide, or slow) is left alone.
//!
//! Brake to drift (`@brake_drift`): in Burnout a tap of the brake at speed
//! starts a drift; GTA needs the handbrake. Pressing the brake at drifting
//! speed (at least `@brake_drift_speed`, moving forward, wheels down) works
//! as the handbrake for the first `@brake_drift_time` seconds; held longer,
//! it's the normal brake again. With the handbrake already held, the brake is
//! always just the brake; pressing the handbrake button during the drift
//! switches to the normal brake (the button then does nothing until the
//! brake is let go).

use crate::natives as n;

#[derive(Clone, Copy)]
pub struct DriftSettings {
    /// How strongly it helps: 0 = off (GTA as it was), 1 = strong, up to 3.
    pub assist: f32,
    /// Largest drift angle (degrees) it lets the slide reach.
    pub max_angle: f32,
    /// Brake taps at speed work as the handbrake.
    pub brake_drift: bool,
    /// For how long (real seconds) a brake press is the handbrake.
    pub brake_drift_time: f32,
    /// From this speed (m/s).
    pub brake_drift_speed: f32,
}

impl Default for DriftSettings {
    fn default() -> Self {
        Self { assist: 1.1, max_angle: 45.0, brake_drift: true, brake_drift_time: 0.5, brake_drift_speed: 2.2 }
    }
}

/// INPUT_VEH_BRAKE and INPUT_VEH_HANDBRAKE.
const CONTROL_BRAKE: i32 = 72;
const CONTROL_HANDBRAKE: i32 = 76;

/// Brake-to-drift state, kept between frames.
#[derive(Default)]
pub struct Drift {
    /// When the current brake press started acting as the handbrake.
    handbrake_since: Option<std::time::Instant>,
    /// The brake is held from a press that has already been dealt with.
    held: bool,
    /// The car the drift handbrake is on (0: none).
    forced: i32,
    /// The handbrake button was pressed during the drift: this brake press
    /// is the normal brake.
    cancelled: bool,
}

impl Drift {
    /// Every frame, for the player's car (0: not driving).
    pub fn update(&mut self, veh: i32, s: &DriftSettings) {
        self.brake_drift(veh, s);
        assist(veh, s);
    }

    /// Lets go of the handbrake this put on.
    fn release(&mut self) {
        if self.forced != 0 {
            n::set_vehicle_handbrake(self.forced, false);
            self.forced = 0;
        }
    }

    fn brake_drift(&mut self, veh: i32, s: &DriftSettings) {
        if veh == 0 || !s.brake_drift || (self.forced != 0 && self.forced != veh) {
            self.release();
            *self = Self::default();
            return;
        }
        // Read the brake even while it's blocked below.
        let pressed = n::get_disabled_control_normal(CONTROL_BRAKE) > 0.25;
        if !pressed {
            self.release();
            *self = Self::default();
            return;
        }
        // The player's own handbrake (the drift handbrake is put on the car
        // directly, not pressed for them, so this is only the player).
        let own_handbrake = n::get_control_normal(CONTROL_HANDBRAKE) > 0.25;
        if !self.held {
            // A new press: the handbrake if going fast enough, forward, on
            // the ground, and the player isn't already on the handbrake;
            // otherwise just the brake.
            self.held = true;
            let v = n::get_entity_speed_vector_relative(veh);
            let ok = !own_handbrake && v.y >= s.brake_drift_speed && !n::is_entity_in_air(veh) && n::is_vehicle_on_all_wheels(veh);
            if ok {
                self.handbrake_since = Some(std::time::Instant::now());
            }
        }
        if self.cancelled {
            // Switched to the normal brake: the handbrake button stays off
            // until the brake is let go.
            n::disable_control_only(CONTROL_HANDBRAKE);
            return;
        }
        if let Some(t) = self.handbrake_since {
            if own_handbrake {
                // The handbrake button during the drift: the normal brake
                // from here instead.
                self.handbrake_since = None;
                self.cancelled = true;
                self.release();
                n::disable_control_only(CONTROL_HANDBRAKE);
            } else if t.elapsed().as_secs_f32() < s.brake_drift_time {
                n::disable_control_only(CONTROL_BRAKE);
                n::set_vehicle_handbrake(veh, true);
                self.forced = veh;
            } else {
                // Held on: the normal brake from here.
                self.handbrake_since = None;
                self.release();
            }
        }
    }
}

/// Only above this speed (m/s, ~54 km/h)...
const MIN_SPEED: f32 = 15.0;
/// ...and with the car this far out of line (degrees) is it a slide.
const SLIDE_ANGLE: f32 = 6.0;
/// Below this drift angle (degrees) the car is coming back straight: the
/// rotation that would carry it into the other direction is damped.
const STRAIGHT_ANGLE: f32 = 15.0;

/// The drift assist, every frame.
fn assist(veh: i32, s: &DriftSettings) {
    if veh == 0 || s.assist <= 0.0 || n::is_entity_in_air(veh) || !n::is_vehicle_on_all_wheels(veh) {
        return;
    }
    // Car space: x to the right, y forward.
    let v = n::get_entity_speed_vector_relative(veh);
    let speed = v.x.hypot(v.y);
    if speed < MIN_SPEED || v.y <= 0.0 {
        return;
    }
    // Drift angle: + when the car is moving to its right of where it points
    // (its nose has swung left), - the other way.
    let angle = v.x.atan2(v.y).to_degrees();
    if angle.abs() < SLIDE_ANGLE {
        return;
    }
    let dt = n::get_frame_time().clamp(0.0, 0.1);
    let w = n::get_entity_rotation_velocity(veh);
    let mut yaw = w.z;
    // Rotation that makes the angle grow has the same sign as the angle
    // (yawing left swings the motion further to the car's right).
    let growing = yaw * angle > 0.0;
    let k = s.assist.clamp(0.0, 3.0);

    if angle.abs() > s.max_angle && growing {
        // Past the limit: stop the rotation and ease it back.
        let over = (angle.abs() - s.max_angle).min(30.0);
        yaw *= (-(6.0 * k) * dt).exp();
        yaw -= angle.signum() * (over.to_radians() * 4.0 * k) * dt * 60.0 * 0.05;
    } else if angle.abs() < STRAIGHT_ANGLE && !growing {
        // Coming back straight: damp the swing so it doesn't flick over.
        yaw *= (-(5.0 * k) * dt).exp();
    } else {
        // In the slide: steadier, still steerable.
        yaw *= (-(1.2 * k) * dt).exp();
    }
    if (yaw - w.z).abs() > 1e-4 {
        n::set_entity_angular_velocity(veh, w.x, w.y, yaw);
    }
}
