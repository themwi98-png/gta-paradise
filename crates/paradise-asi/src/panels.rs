//! Loose panels: the player's car's bonnet or boot, once a crash has
//! unlatched it and it swings open (more than OPEN of the way), is broken
//! off so it falls away instead of flapping over the windscreen. Neither
//! is ever opened by the player (only side doors are, getting in and out),
//! so an open one is a damaged one. Logged.

use crate::natives as n;

/// This much of the way open (0-1) counts as swinging loose.
const OPEN: f32 = 0.1;
/// Bonnet and boot (SET_VEHICLE_DOOR_SHUT's ids).
const PANELS: [(i32, &str); 2] = [(4, "bonnet"), (5, "boot")];

/// Every frame, for the car the player is driving.
pub fn update(car: i32) {
    if car == 0 || !n::does_entity_exist(car) {
        return;
    }
    for (door, name) in PANELS {
        if !n::get_is_door_valid(car, door) || n::is_vehicle_door_damaged(car, door) {
            continue;
        }
        let open = n::get_vehicle_door_angle_ratio(car, door);
        if open > OPEN {
            n::set_vehicle_door_broken(car, door, false);
            crate::log(&format!("panels: {name} came loose ({:.0}% open) at {:.0} m/s; broken off", open * 100.0, n::get_entity_speed(car)));
        }
    }
}
