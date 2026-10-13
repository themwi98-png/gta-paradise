//! Keeps Los Santos leftovers from spawning in Paradise City.
//!
//! Some of GTA's own spawns still use Los Santos positions: parked-car
//! generators, scenario spots (NPC groups, parked helicopters and planes)
//! and scripts. In Paradise those positions are mid-air or inside
//! buildings. Twice a second, every NPC and vehicle that has just appeared
//! is checked:
//! - one that appears more than AIR_HEIGHT above the ground (or over no
//!   ground at all) is removed; flying aircraft with their engine running
//!   (police helicopters) are left alone;
//! - one that appears inside a no-spawn zone is removed. Zones come from
//!   `paradise_spawn_blocks.json` (game folder; placed with the viewer's
//!   Spawns panel), and GTA's own parked-car generators and scenario spots
//!   are switched off inside them too.
//!
//! GTA's trains are switched off too: their tracks run through Los Santos,
//! which in Paradise is the sky.
//!
//! Everything removed is logged to `paradise_spawns.csv` (what it was, where,
//! how it was spawned) so the source can be found and taken out.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::HashSet;
use std::io::Write as _;

const ZONES_FILE: &str = "paradise_spawn_blocks.json";
const LOG_FILE: &str = "paradise_spawns.csv";
/// Appearing this high (m) above the ground counts as a mid-air spawn.
const AIR_HEIGHT: f32 = 8.0;
/// Zones reach this far up and down (m) from their centre, at least.
const ZONE_HEIGHT: f32 = 40.0;
const SCAN_MS: i32 = 500;
/// At most this many new NPCs and vehicles are checked per scan (each check
/// traces a line to the ground); the rest wait for the next scan, so a crowd
/// spawning at once doesn't stall a frame.
const CHECKS_PER_SCAN: usize = 16;
/// GTA's train tracks (track ids 0..TRAIN_TRACKS).
const TRAIN_TRACKS: i32 = 28;

pub struct SpawnGuard {
    /// x, y, z, radius (GTA world).
    zones: Vec<[f32; 4]>,
    seen: HashSet<i32>,
    last_scan: i32,
    applied: bool,
    log: Option<std::fs::File>,
}

/// The numbers in the `"zones"` list of the zones file, in fours.
fn read_zones() -> Vec<[f32; 4]> {
    let Ok(text) = std::fs::read_to_string(ZONES_FILE) else { return Vec::new() };
    let Some(at) = text.find("\"zones\"") else { return Vec::new() };
    let nums: Vec<f32> = text[at + 7..]
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+'))
        .filter_map(|t| t.parse().ok())
        .collect();
    nums.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
}

impl SpawnGuard {
    pub fn load() -> Self {
        let zones = read_zones();
        crate::log(&format!("spawn guard: {} no-spawn zone(s)", zones.len()));
        let mut log = std::fs::File::create(LOG_FILE).ok();
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "t,kind,model,x,y,z,height,population,script,reason");
        }
        Self { zones, seen: HashSet::new(), last_scan: 0, applied: false, log }
    }

    /// Switches GTA's own parked-car generators and scenario spots off in
    /// the zones (once the Paradise map is in).
    fn apply_zones(&mut self) {
        self.applied = true;
        for track in 0..TRAIN_TRACKS {
            n::switch_train_track(track, false);
        }
        for z in &self.zones {
            let a = Vec3::new(z[0] - z[3], z[1] - z[3], z[2] - ZONE_HEIGHT.max(z[3]));
            let b = Vec3::new(z[0] + z[3], z[1] + z[3], z[2] + ZONE_HEIGHT.max(z[3]));
            n::add_scenario_blocking_area(a, b);
            n::set_vehicle_generators_active_in_area(a, b, false);
            n::remove_vehicles_from_generators_in_area(a, b);
        }
    }

    fn in_zone(&self, p: Vec3) -> bool {
        self.zones.iter().any(|z| (p.x - z[0]).powi(2) + (p.y - z[1]).powi(2) <= z[3] * z[3] && (p.z - z[2]).abs() <= ZONE_HEIGHT.max(z[3]))
    }

    /// Height above the ground below `p` (None: no ground at all).
    fn height(p: Vec3, ignore: i32) -> Option<f32> {
        n::raycast(Vec3::new(p.x, p.y, p.z + 0.5), Vec3::new(p.x, p.y, p.z - 500.0), 1, ignore).map(|h| p.z - h.pos.z)
    }

    pub fn update(&mut self, player_ped: i32, now: i32) {
        if !self.applied {
            self.apply_zones();
        }
        if now - self.last_scan < SCAN_MS {
            return;
        }
        self.last_scan = now;
        // No trains (switched off again in case a script turns them on).
        n::set_random_trains(false);
        n::delete_all_trains();
        let mine = if n::is_ped_in_any_vehicle(player_ped) { n::get_vehicle_ped_is_in(player_ped) } else { 0 };
        let mut checks = 0;
        let vehicles = shv::all_vehicles();
        let peds = shv::all_peds();
        let mut present: HashSet<i32> = HashSet::with_capacity(vehicles.len() + peds.len());
        for &v in &vehicles {
            present.insert(v);
            if self.seen.contains(&v) || v == mine || checks >= CHECKS_PER_SCAN {
                continue;
            }
            self.seen.insert(v);
            checks += 1;
            let p = n::get_entity_coords(v);
            let flying = matches!(n::get_vehicle_class(v), 15 | 16) && n::get_is_vehicle_engine_running(v);
            let reason = if self.in_zone(p) {
                Some("no-spawn zone")
            } else if !flying && !n::is_entity_in_water(v) && Self::height(p, v).is_none_or(|h| h > AIR_HEIGHT) {
                Some("appeared in mid-air")
            } else {
                None
            };
            if let Some(why) = reason {
                self.record("vehicle", v, p, why);
                // Anyone inside goes too (the player is never in these).
                for seat in -1..8 {
                    let occupant = n::get_ped_in_vehicle_seat(v, seat);
                    if occupant != 0 && occupant != player_ped {
                        n::delete_ped(occupant);
                    }
                }
                n::delete_vehicle(v);
            }
        }
        for &p in &peds {
            present.insert(p);
            // (Occupants of removed vehicles are already gone.)
            if self.seen.contains(&p) || checks >= CHECKS_PER_SCAN {
                continue;
            }
            self.seen.insert(p);
            if p == player_ped || !n::does_entity_exist(p) || n::is_ped_a_player(p) || n::is_ped_in_any_vehicle(p) {
                continue;
            }
            checks += 1;
            let at = n::get_entity_coords(p);
            let reason = if self.in_zone(at) {
                Some("no-spawn zone")
            } else if !n::is_entity_in_water(p) && Self::height(at, p).is_none_or(|h| h > AIR_HEIGHT) {
                Some("appeared in mid-air")
            } else {
                None
            };
            if let Some(why) = reason {
                self.record("ped", p, at, why);
                n::delete_ped(p);
            }
        }
        // Forget what's gone (handles get reused).
        self.seen.retain(|h| present.contains(h));
    }

    fn record(&mut self, kind: &str, e: i32, p: Vec3, why: &str) {
        let height = Self::height(p, e).map_or("none".to_string(), |h| format!("{h:.1}"));
        let script = n::get_entity_script(e).unwrap_or_default();
        let line = format!(
            "{:.1},{kind},{},{:.1},{:.1},{:.1},{height},{},{script},{why}",
            n::get_game_timer() as f32 / 1000.0,
            n::get_entity_model(e),
            p.x,
            p.y,
            p.z,
            n::get_entity_population_type(e)
        );
        crate::log(&format!("SPAWN REMOVED: {line}"));
        if let Some(f) = self.log.as_mut() {
            let _ = writeln!(f, "{line}");
        }
    }
}
