//! Burnout-style vehicle physics on top of GTA's, for vehicles near the
//! player (not aircraft, boats or trains).
//!
//! - Jumps: GTA cars hang in the air off jumps; Burnout's drop much sooner.
//!   Airborne vehicles get extra downward pull, so gravity on them is
//!   `@air_gravity` times normal (1 = GTA as it was).
//! - Downforce: on the ground, fast cars are pressed down harder, growing
//!   with the square of speed like real downforce: `@downforce` g extra at
//!   DOWNFORCE_SPEED (0 = none), so they stay planted at Burnout speeds.

use crate::natives as n;
use crate::shv;

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
