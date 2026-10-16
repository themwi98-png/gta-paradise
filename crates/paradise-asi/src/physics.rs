//! Burnout-style vehicle physics on top of GTA's, for vehicles near the
//! player (not aircraft, boats or trains).
//!
//! - Jumps: GTA cars hang in the air off jumps; Burnout's drop much sooner.
//!   Airborne vehicles get extra downward pull, so gravity on them is
//!   `@air_gravity` times normal (1 = GTA as it was).
//! - Downforce: on the ground, fast cars are pressed down harder, growing
//!   with the square of speed like real downforce: `@downforce` g extra at
//!   DOWNFORCE_SPEED (0 = none), so they stay planted at Burnout speeds.
//! - Speed and power (`CarTuning`): the installed handling makes every car
//!   BAKED_TOP times GTA's top speed and BAKED_POWER times its engine power;
//!   `@car_top_speed` and `@car_power` change that live, as totals against
//!   GTA (so the defaults, equal to the installed values, change nothing).
//!   Police chasing you keep their own tuning (chase.rs).

use crate::natives as n;
use crate::shv;
use std::collections::HashMap;

const G: f32 = 9.81;
/// Vehicles further than this from the player are left alone (m).
const RANGE: f32 = 250.0;
/// Speed (m/s, ~180 km/h) at which downforce adds `@downforce` g.
const DOWNFORCE_SPEED: f32 = 50.0;
/// No downforce below this speed (m/s), and never more than this many g.
const DOWNFORCE_MIN_SPEED: f32 = 12.0;
const DOWNFORCE_MAX_G: f32 = 3.0;

pub fn vehicle_forces(air_gravity: f32, downforce: f32, player_pos: shv::Vec3) {
    if air_gravity <= 1.0 && downforce <= 0.0 {
        return;
    }
    let dt = n::get_frame_time().clamp(0.0, 0.1);
    for veh in shv::all_vehicles() {
        if matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) {
            continue;
        }
        let p = n::get_entity_coords(veh);
        let (dx, dy) = (p.x - player_pos.x, p.y - player_pos.y);
        if dx * dx + dy * dy > RANGE * RANGE {
            continue;
        }
        let extra_g = if n::is_entity_in_air(veh) {
            air_gravity - 1.0
        } else {
            let speed = n::get_entity_speed(veh);
            if speed < DOWNFORCE_MIN_SPEED {
                continue;
            }
            (downforce * (speed / DOWNFORCE_SPEED).powi(2)).min(DOWNFORCE_MAX_G)
        };
        if extra_g <= 0.0 {
            continue;
        }
        let v = n::get_entity_velocity(veh);
        n::set_entity_velocity(veh, v.x, v.y, v.z - extra_g * G * dt);
    }
}

/// Top speed and engine power the installed handling gives cars, times
/// GTA's (fInitialDriveMaxFlatVel and fInitialDriveForce in bpx's
/// gtadata.rs HANDLING).
pub const BAKED_TOP: f32 = 1.35;
pub const BAKED_POWER: f32 = 1.35;

/// Applies `@car_top_speed` / `@car_power` to vehicles near the player.
#[derive(Default)]
pub struct CarTuning {
    /// Top-speed change (percent) applied to each vehicle so far.
    top_applied: HashMap<i32, f32>,
}

impl CarTuning {
    /// Every frame. `top` and `power` are totals against GTA's.
    pub fn update(&mut self, top: f32, power: f32, player_pos: shv::Vec3) {
        let percent = ((top / BAKED_TOP - 1.0) * 100.0).clamp(-60.0, 300.0);
        let power = power / BAKED_POWER;
        let all = shv::all_vehicles();
        self.top_applied.retain(|v, _| all.contains(v));
        if percent.abs() < 0.5 && (power - 1.0).abs() < 0.01 && self.top_applied.is_empty() {
            return;
        }
        for veh in all {
            if matches!(n::get_vehicle_class(veh), 14 | 15 | 16 | 21) {
                continue;
            }
            let p = n::get_entity_coords(veh);
            if (p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2) > RANGE * RANGE {
                continue;
            }
            // Police drivers have their own speed and power (chase.rs).
            let driver = n::get_ped_in_vehicle_seat(veh, -1);
            if driver != 0 && !n::is_ped_a_player(driver) && matches!(n::get_ped_type(driver), 6 | 27 | 29) {
                continue;
            }
            if self.top_applied.get(&veh).is_none_or(|&a| (a - percent).abs() >= 0.5) {
                n::modify_vehicle_top_speed(veh, percent);
                self.top_applied.insert(veh, percent);
            }
            // Engine power only lasts the frame it's set in.
            if (power - 1.0).abs() >= 0.01 {
                n::set_vehicle_cheat_power_increase(veh, power);
            }
        }
    }
}
