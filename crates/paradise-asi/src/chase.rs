//! Police chase recorder: while the player is wanted, once a second, every
//! police-driven vehicle within 800 m is written to `paradise_chase.csv`
//! (position, speed, distance to the player, distance to the nearest road
//! node and whether GTA considers it stuck), so places where pursuers lose
//! the player can be found on the map afterwards.
//!
//! GTA normally keeps road nodes loaded only near the player, so police
//! dispatched from further out had no roads to route on (they drifted away
//! or left their cars to run); all of Paradise City's nodes (~15 000) are
//! kept loaded instead.
//!
//! It also makes pursuers a match for a boosting Burnout car: police drivers
//! near the player get top driving skill and aggression, extra top speed
//! (`@police_top`, percent) and extra engine power (`@police_power`). They
//! stay in their cars while the player is driving (GTA had them jump out as
//! the player sped past) and still get out to chase a player on foot. A
//! police car with a clear view of the player within SPOT_RANGE reports
//! them, so the search doesn't go cold with the player in plain sight.
//!
//! At those speeds GTA's police kept crashing themselves out of the chase,
//! so (`@police_tough`) their cars don't catch fire, keep at least
//! ENGINE_FLOOR engine health (smashed up but still driving) and have
//! strong bodies and axles, and their drivers can't be killed by a crash or
//! thrown out of the car.
//!
//! Police route along the road nodes, which only exist where Burnout has
//! traffic lanes. When the player drives somewhere without lanes (more than
//! OFFROAD m from a node), police cars within ASSIST_RANGE are sent straight
//! at the player instead (by road until within STRAIGHT m), and handed back
//! to GTA's own pursuit once the player is back on the roads.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::HashSet;
use std::io::Write as _;

const FILE: &str = "paradise_chase.csv";

/// `@police...` settings from paradise.ini.
#[derive(Clone, Copy)]
pub struct PoliceSettings {
    /// Engine power multiplier for pursuing police (1 = as GTA).
    pub power: f32,
    /// Extra top speed for pursuing police, percent.
    pub top: f32,
    /// Police cars and drivers survive their own crashes.
    pub tough: bool,
}

impl Default for PoliceSettings {
    fn default() -> Self {
        Self { power: 1.0, top: 100.0, tough: true }
    }
}

/// Police drivers near the player (within this, m) are tuned.
const TUNE_RANGE: f32 = 600.0;
/// Tough police cars never drop below this engine health (GTA: 1000 new,
/// smoking below ~400, on fire below 0).
const ENGINE_FLOOR: f32 = 250.0;
/// A police car this close (m) with a clear line of sight sees the player.
const SPOT_RANGE: f32 = 120.0;
/// The player this far (m) from any road node is off the road network.
const OFFROAD: f32 = 40.0;
/// Police cars this close (m) are sent after an off-road player...
const ASSIST_RANGE: f32 = 450.0;
/// ...driving straight at them from this close (m).
const STRAIGHT: f32 = 250.0;
/// Re-aimed at the player's new position this often (ms).
const ASSIST_MS: i32 = 1500;
/// Aggressive driving: through red lights, around traffic.
const ASSIST_STYLE: i32 = 786_468;

pub struct Chase {
    tuned: HashSet<i32>,
    /// Whether each tuned driver may leave its car, as last set.
    may_leave: std::collections::HashMap<i32, bool>,
    last_spot: i32,
    last_nodes: i32,
    nodes_logged: bool,
    file: Option<std::fs::File>,
    last: i32,
    chases: u32,
    was_wanted: bool,
    /// Police drivers currently sent straight at an off-road player.
    assisted: HashSet<i32>,
    last_assist: i32,
}

impl Chase {
    pub fn new() -> Self {
        let mut file = std::fs::File::create(FILE).ok();
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "t,chase,wanted,px,py,pz,pspeed,cop,model,x,y,z,speed,dist,node_dist,stuck");
        }
        Self { tuned: HashSet::new(), may_leave: Default::default(), last_spot: 0, last_nodes: -100_000, nodes_logged: false, file, last: 0, chases: 0, was_wanted: false, assisted: HashSet::new(), last_assist: 0 }
    }

    /// Every frame: police pursuers' extra power (it only lasts a frame).
    fn pursue(&mut self, s: &PoliceSettings, ped: i32, player_pos: Vec3, now: i32) {
        let player_driving = n::is_ped_in_any_vehicle(ped);
        let check_sight = now - self.last_spot >= 500;
        let mut spotted = false;
        // Off the road network? Then nearby police drive straight at the
        // player; back on it, they go back to GTA's pursuit.
        let assist_now = now - self.last_assist >= ASSIST_MS;
        let off_road = assist_now
            && player_driving
            && n::closest_vehicle_node(player_pos).is_none_or(|(q, _)| (q.x - player_pos.x).hypot(q.y - player_pos.y) > OFFROAD);
        if assist_now {
            self.last_assist = now;
            if !off_road && !self.assisted.is_empty() {
                for &driver in &self.assisted {
                    if n::does_entity_exist(driver) && n::is_ped_in_any_vehicle(driver) {
                        n::task_vehicle_chase(driver, ped);
                    }
                }
                crate::log(&format!("police: player back on the roads, {} car(s) back to pursuit", self.assisted.len()));
                self.assisted.clear();
            }
        }
        for veh in shv::all_vehicles() {
            let driver = n::get_ped_in_vehicle_seat(veh, -1);
            if driver == 0 || n::is_ped_a_player(driver) || !matches!(n::get_ped_type(driver), 6 | 27 | 29) {
                continue;
            }
            let p = n::get_entity_coords(veh);
            if (p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2) > TUNE_RANGE * TUNE_RANGE {
                continue;
            }
            if self.may_leave.get(&driver) != Some(&!player_driving) {
                n::set_ped_combat_attribute(driver, 3, !player_driving);
                self.may_leave.insert(driver, !player_driving);
            }
            if off_road && (p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2) < ASSIST_RANGE * ASSIST_RANGE {
                if self.assisted.insert(driver) {
                    crate::log(&format!("police: player off the roads, car {veh} sent straight at them"));
                }
                n::task_vehicle_drive_to_coord(driver, veh, player_pos, 60.0, ASSIST_STYLE, 3.0, STRAIGHT);
            }
            if check_sight && !spotted {
                let d2 = (p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2);
                spotted = d2 < SPOT_RANGE * SPOT_RANGE && n::has_clear_los(driver, ped);
            }
            if self.tuned.insert(veh) {
                n::set_ped_seeing_range(driver, SPOT_RANGE);
                n::set_driver_ability(driver, 1.0);
                n::set_driver_aggressiveness(driver, 1.0);
                n::set_driver_racing_modifier(driver, 1.0);
                if s.top > 0.0 {
                    n::modify_vehicle_top_speed(veh, s.top);
                }
                if s.tough {
                    n::set_vehicle_strong(veh, true);
                    n::set_vehicle_has_strong_axles(veh, true);
                    n::set_disable_vehicle_fires(veh, true);
                    n::set_collision_proof(driver, true);
                    n::set_ped_config_flag(driver, 32, false); // through the windscreen
                    n::set_ped_can_be_knocked_off_vehicle(driver, 1);
                }
            }
            if s.tough && n::get_vehicle_engine_health(veh) < ENGINE_FLOOR {
                n::set_vehicle_engine_health(veh, ENGINE_FLOOR);
            }
            if s.power > 1.0 {
                n::set_vehicle_cheat_power_increase(veh, s.power);
            }
        }
        if check_sight {
            self.last_spot = now;
            if spotted {
                let player = n::player_id();
                n::report_police_spotted_player(player);
                n::set_player_wanted_centre(player, player_pos);
            }
        }
    }

    pub fn update(&mut self, s: &PoliceSettings, ped: i32, player_pos: Vec3, now: i32) {
        // Keep every road node loaded (re-asserted now and then, cheap).
        if now - self.last_nodes > 5000 {
            self.last_nodes = now;
            let all = n::load_all_path_nodes(true);
            if !self.nodes_logged && all {
                self.nodes_logged = true;
                crate::log(&format!(
                    "road nodes: all loaded; Paradise area loaded = {}",
                    n::are_nodes_loaded_for_area(-5000.0, -4000.0, 7900.0, 8000.0)
                ));
            }
        }
        if n::get_player_wanted_level(n::player_id()) > 0 {
            self.pursue(s, ped, player_pos, now);
        } else if !self.tuned.is_empty() {
            self.tuned.clear();
            self.may_leave.clear();
            self.assisted.clear();
        }
        if now - self.last < 1000 {
            return;
        }
        self.last = now;
        let wanted = n::get_player_wanted_level(n::player_id());
        if wanted == 0 {
            self.was_wanted = false;
            return;
        }
        if !self.was_wanted {
            self.was_wanted = true;
            self.chases += 1;
        }
        let Some(f) = self.file.as_mut() else { return };
        let me = if n::is_ped_in_any_vehicle(ped) { n::get_vehicle_ped_is_in(ped) } else { ped };
        let pspeed = n::get_entity_speed(me);
        let mut cops = 0;
        for veh in shv::all_vehicles() {
            if veh == me {
                continue;
            }
            let driver = n::get_ped_in_vehicle_seat(veh, -1);
            if driver == 0 || !matches!(n::get_ped_type(driver), 6 | 27 | 29) {
                continue;
            }
            let p = n::get_entity_coords(veh);
            let dist = ((p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2) + (p.z - player_pos.z).powi(2)).sqrt();
            if dist > 800.0 {
                continue;
            }
            let node = n::closest_vehicle_node(p).map(|(q, _)| ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt()).unwrap_or(-1.0);
            let stuck = (0..4).filter(|&k| n::is_vehicle_stuck_timer_up(veh, k, 3000)).fold(0, |a, k| a | (1 << k));
            let _ = writeln!(
                f,
                "{:.1},{},{wanted},{:.1},{:.1},{:.1},{:.1},{veh},{},{:.1},{:.1},{:.1},{:.1},{:.0},{:.1},{stuck}",
                now as f32 / 1000.0,
                self.chases,
                player_pos.x,
                player_pos.y,
                player_pos.z,
                pspeed,
                n::get_entity_model(veh),
                p.x,
                p.y,
                p.z,
                n::get_entity_speed(veh),
                dist,
                node
            );
            cops += 1;
        }
        if cops == 0 {
            let _ = writeln!(f, "{:.1},{},{wanted},{:.1},{:.1},{:.1},{:.1},0,0,,,,,,,", now as f32 / 1000.0, self.chases, player_pos.x, player_pos.y, player_pos.z, pspeed);
        }
    }
}
