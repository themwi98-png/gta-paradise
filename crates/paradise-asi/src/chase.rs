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
//! the player sped past) and still get out to chase a player on foot.
//!
//! GTA's police lose the player (the stars flash and they search around
//! where the player was last seen) once none has seen them for a moment,
//! and they see only so far: a Burnout car is out of sight in seconds. So
//! while any police vehicle is within `@police_sight` m, seen or not, the
//! police are told where the player is; only further away can the player
//! get away.
//!
//! At those speeds GTA's police kept crashing themselves out of the chase,
//! so (`@police_tough`) their cars can't be damaged at all until the
//! player's car hits them (takedown.rs then makes them breakable for as
//! long as they're vulnerable), don't catch fire, keep at least
//! ENGINE_FLOOR engine health and have strong bodies and axles, and their
//! drivers can't be killed by a crash or thrown out of the car.
//!
//! Even so they slid off at those speeds, so (`@police_grip`) their cars
//! go where they point, physics or not: on the ground, each frame that
//! share of the sideways slide becomes forward speed (1 = no sliding at
//! all), their turning is held to what GRIP_MAX_G allows at their speed so
//! they can't spin, and one left on its side or roof is put back on its
//! wheels, and one stuck there (or hung up, or jammed) is put back on the
//! nearest road facing the player, as the player is after a wreck. A police
//! car the player's car hits is left to GTA's physics for
//! HIT_GRACE, and for as long as it is vulnerable or once taken down
//! (takedown.rs), so it can still be knocked out of the chase.
//!
//! Their speed is helped the same way. What GTA's physics does to a police
//! car's speed each frame shows what its driver is doing: braking hard,
//! it slows `@police_brakes` times as fast (never sliding, with the grip
//! above, nor rolling backwards); on the throttle, it gains up to
//! `@police_accel` g more; and with the player ahead (within CATCHUP_CONE,
//! further than CATCHUP_MIN), it gets all of that until it's going the
//! player's speed plus up to CATCHUP_MARGIN, so a boosting player is caught.
//! Their drive tasks get a CRUISE m/s cruise speed, so GTA's own driving
//! doesn't hold them back.
//!
//! And they try to take the player down, as Burnout's rivals do
//! (`@police_ram`): a police car within RAM_RANGE going the player's way,
//! behind or alongside, goes for the player's car: its nose swung at where
//! the player will be (the rear corner on its side from behind, a PIT; the
//! player's side from alongside, a slam), and up to `@police_ram` m/s
//! faster, its own braking ignored. One coming the other way within
//! ONCOMING_RANGE (`@police_ram_oncoming`) steers into the player's path
//! for a head-on. `@police_rammers` at a time, each for up to RAM_TIME,
//! then RAM_COOLDOWN before it tries again.
//!
//! Police route along the road nodes, which only exist where Burnout has
//! traffic lanes. When the player drives somewhere without lanes (more than
//! OFFROAD m from a node), police cars within ASSIST_RANGE are sent straight
//! at the player instead (by road until within STRAIGHT m), and handed back
//! to GTA's own pursuit once the player is back on the roads.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::collections::{HashMap, HashSet};
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
    /// Share of a police car's sideways slide taken away each frame (0 =
    /// GTA's physics, 1 = none at all).
    pub grip: f32,
    /// Police vehicles this close (m) always know where the player is (0 =
    /// only what GTA's police see).
    pub sight: f32,
    /// Extra acceleration (g) when speeding up or catching up (0 = GTA's).
    pub accel: f32,
    /// Braking, times GTA's (1 = GTA's).
    pub brakes: f32,
    /// How much faster (m/s) a police car goes for the player's car when
    /// ramming it (0 = no ramming).
    pub ram: f32,
    /// Police cars coming the other way ram too.
    pub ram_oncoming: bool,
    /// How many police cars may be going for the player at once.
    pub rammers: usize,
}

impl Default for PoliceSettings {
    fn default() -> Self {
        Self { power: 1.0, top: 100.0, tough: true, grip: 1.0, sight: 400.0, accel: 1.0, brakes: 3.0, ram: 12.0, ram_oncoming: true, rammers: 4 }
    }
}

/// A police car's right, forward and up directions (world).
type Axes = ([f32; 3], [f32; 3], [f32; 3]);

/// A police car's forward speed as this plugin left it last frame, and the
/// acceleration GTA's physics has been giving it (smoothed).
#[derive(Clone, Copy)]
struct Motion {
    fwd: f32,
    accel: f32,
    at: i32,
}

/// Police drivers near the player (within this, m) are tuned.
const TUNE_RANGE: f32 = 600.0;
/// Tough police cars never drop below this engine health (GTA: 1000 new,
/// smoking below ~400, on fire below 0).
const ENGINE_FLOOR: f32 = 250.0;
/// GTA's own sight for police drivers reaches at least this far (m).
const SEEING_RANGE: f32 = 120.0;
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
/// Grip only helps police cars going forward faster than this (m/s)...
const GRIP_MIN_SPEED: f32 = 6.0;
/// ...and never lets them turn harder than this sideways pull allows (g).
const GRIP_MAX_G: f32 = 3.0;
/// A police car the player's car touches is left to GTA's physics this
/// long (ms).
const HIT_GRACE: i32 = 1500;
/// A police car on its side or roof this long (ms) is put back on its wheels.
const RIGHT_AFTER: i32 = 1000;
const G: f32 = 9.81;
/// Slowing faster than this (m/s²) is the driver braking...
const BRAKE_DETECT: f32 = 3.0;
/// ...slower than this, coasting (drag, a slope)...
const COAST: f32 = 1.5;
/// ...and speeding up faster than this, on the throttle; at FULL_THROTTLE
/// it gets all of `@police_accel`.
const ACCEL_DETECT: f32 = 0.5;
const FULL_THROTTLE: f32 = 4.0;
/// A speed change faster than this (m/s²) is a knock, not driving.
const KNOCK: f32 = 30.0;
/// Catching up: the player further ahead than this (m)...
const CATCHUP_MIN: f32 = 30.0;
/// ...within 60 degrees of where the car points (cosine)...
const CATCHUP_CONE: f32 = 0.5;
/// ...makes it go the player's speed plus a tenth of the gap, up to this
/// much more (m/s).
const CATCHUP_MARGIN: f32 = 20.0;
/// Never pushed faster than this (m/s, ~340 km/h).
const MAX_SPEED: f32 = 95.0;
/// Cruise speed given to their drive tasks (m/s).
const CRUISE: f32 = 80.0;
/// A police car hung up or jammed this long (ms) is stuck...
const STUCK_MS: i32 = 3000;
/// ...and put back on a road at least this far (m) from the player, at this
/// speed (m/s), at most once every MOVE_AGAIN ms.
const MOVE_CLEAR: f32 = 15.0;
const MOVE_SPEED: f32 = 12.0;
const MOVE_AGAIN: i32 = 5000;
/// Ramming: police cars this close (m) to the player's car...
const RAM_RANGE: f32 = 45.0;
/// ...going the same way (cosine) and at least this fast (m/s), as the
/// player is...
const RAM_SAME_WAY: f32 = 0.5;
const RAM_MIN_SPEED: f32 = 8.0;
/// ...and not further ahead than this (m).
const RAM_AHEAD: f32 = 4.0;
/// Each going for it at most this long (ms), then waiting this long (ms)
/// before trying again.
const RAM_TIME: i32 = 2500;
const RAM_COOLDOWN: i32 = 1200;
/// Oncoming police cars this close (m) ahead of the player, and at most
/// ONCOMING_SIDE (m) to the side of its path, go for a head-on.
const ONCOMING_RANGE: f32 = 90.0;
const ONCOMING_SIDE: f32 = 20.0;
/// It speeds up at this (m/s²), turns towards its target at RAM_TURN times
/// the angle off (per second), never harder than RAM_MAX_G sideways, and
/// gives up if the target is more than RAM_GIVE_UP (cosine) off its nose.
const RAM_ACCEL: f32 = 20.0;
const RAM_TURN: f32 = 4.0;
const RAM_MAX_G: f32 = 4.0;
const RAM_GIVE_UP: f32 = 0.25;

/// The player's car, for police cars going for it.
#[derive(Clone, Copy)]
struct Target {
    pos: Vec3,
    vel: Vec3,
    /// Flat forward and right.
    fwd: (f32, f32),
    right: (f32, f32),
    speed: f32,
}

/// What a ramming police car goes for: speed (m/s) and flat direction.
type Ram = Option<(f32, (f32, f32))>;

pub struct Chase {
    tuned: HashSet<i32>,
    /// Whether each tuned driver may leave its car, as last set.
    may_leave: HashMap<i32, bool>,
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
    /// When the player's car last touched each police car.
    hit: HashMap<i32, i32>,
    /// Since when each police car has been on its side or roof.
    flipped: HashMap<i32, i32>,
    motion: HashMap<i32, Motion>,
    /// When each police car was last put back on a road.
    moved: HashMap<i32, i32>,
    /// Police cars made undamageable (`@police_tough`).
    armoured: HashSet<i32>,
    /// Police cars going for the player's car: since when, and whether
    /// head-on.
    rams: HashMap<i32, (i32, bool)>,
    /// When each may go for it again.
    ram_after: HashMap<i32, i32>,
}

impl Chase {
    pub fn new() -> Self {
        let mut file = std::fs::File::create(FILE).ok();
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "t,chase,wanted,px,py,pz,pspeed,cop,model,x,y,z,speed,dist,node_dist,stuck");
        }
        Self {
            tuned: HashSet::new(),
            may_leave: Default::default(),
            last_spot: 0,
            last_nodes: -100_000,
            nodes_logged: false,
            file,
            last: 0,
            chases: 0,
            was_wanted: false,
            assisted: HashSet::new(),
            last_assist: 0,
            hit: HashMap::new(),
            flipped: HashMap::new(),
            motion: HashMap::new(),
            moved: HashMap::new(),
            armoured: HashSet::new(),
            rams: HashMap::new(),
            ram_after: HashMap::new(),
        }
    }

    /// Every frame: police pursuers' extra power (it only lasts a frame).
    fn pursue(&mut self, s: &PoliceSettings, ped: i32, player_pos: Vec3, now: i32, hands_off: &dyn Fn(i32) -> bool) {
        let player_driving = n::is_ped_in_any_vehicle(ped);
        let player_veh = if player_driving { n::get_vehicle_ped_is_in(ped) } else { 0 };
        let player_speed = n::get_entity_speed(if player_veh != 0 { player_veh } else { ped });
        let target = (player_veh != 0).then(|| {
            let vel = n::get_entity_velocity(player_veh);
            let f = n::get_entity_forward_vector(player_veh);
            let l = f.x.hypot(f.y).max(1e-3);
            let fwd = (f.x / l, f.y / l);
            Target { pos: player_pos, vel, fwd, right: (fwd.1, -fwd.0), speed: vel.x.hypot(vel.y) }
        });
        self.rams.retain(|&v, &mut (since, _)| now - since <= RAM_TIME && n::does_entity_exist(v));
        let dt = n::get_frame_time().clamp(0.0, 0.1);
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
            let d2 = (p.x - player_pos.x).powi(2) + (p.y - player_pos.y).powi(2);
            // Police this close keep track of the player, seen or not.
            spotted |= check_sight && d2 < s.sight * s.sight;
            if d2 > TUNE_RANGE * TUNE_RANGE {
                continue;
            }
            // Vulnerable after the player's hit, or taken down
            // (takedown.rs): GTA's physics only.
            if hands_off(veh) {
                if self.armoured.remove(&veh) {
                    n::set_entity_invincible(veh, false);
                }
                self.motion.remove(&veh);
                self.flipped.remove(&veh);
                self.rams.remove(&veh);
                continue;
            }
            // Undamageable until the player's car hits it.
            if s.tough && self.armoured.insert(veh) {
                n::set_entity_invincible(veh, true);
            }
            if self.may_leave.get(&driver) != Some(&!player_driving) {
                n::set_ped_combat_attribute(driver, 3, !player_driving);
                self.may_leave.insert(driver, !player_driving);
            }
            if off_road && d2 < ASSIST_RANGE * ASSIST_RANGE {
                if self.assisted.insert(driver) {
                    crate::log(&format!("police: player off the roads, car {veh} sent straight at them"));
                }
                n::task_vehicle_drive_to_coord(driver, veh, player_pos, 60.0, ASSIST_STYLE, 3.0, STRAIGHT);
            }
            if self.tuned.insert(veh) {
                n::set_ped_seeing_range(driver, s.sight.max(SEEING_RANGE));
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
            if assist_now && s.accel > 0.0 {
                n::set_drive_task_cruise_speed(driver, CRUISE);
            }
            if s.grip > 0.0 || s.accel > 0.0 || s.brakes > 1.0 {
                if player_veh != 0 && n::is_entity_touching_entity(veh, player_veh) {
                    self.hit.insert(veh, now);
                    // Got there: done ramming for now.
                    if self.rams.remove(&veh).is_some() {
                        self.ram_after.insert(veh, now + RAM_COOLDOWN);
                    }
                }
                if self.hit.get(&veh).is_none_or(|&t| now - t > HIT_GRACE) {
                    let m = n::get_entity_matrix(veh);
                    let ram = match target {
                        Some(t) if s.ram > 0.0 => self.ram(veh, s, p, m.1, &t, now),
                        _ => None,
                    };
                    self.control(veh, s, dt, m, p, (player_pos, player_speed), ram, now);
                    self.unstick(veh, m, p, player_pos, now);
                } else {
                    self.motion.remove(&veh);
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
                n::update_wanted_position_this_frame(player);
                n::set_player_wanted_centre(player, player_pos);
            }
        }
    }

    /// Whether police car `veh` (at `p`, pointing `f`) is going for the
    /// player's car, starting or ending a go as it comes: the speed and flat
    /// direction it's going for.
    fn ram(&mut self, veh: i32, s: &PoliceSettings, p: Vec3, f: [f32; 3], t: &Target, now: i32) -> Ram {
        let (dx, dy) = (p.x - t.pos.x, p.y - t.pos.y);
        let dist = dx.hypot(dy);
        let ahead = dx * t.fwd.0 + dy * t.fwd.1;
        let beside = dx * t.right.0 + dy * t.right.1;
        let fl = f[0].hypot(f[1]).max(1e-3);
        let (fx, fy) = (f[0] / fl, f[1] / fl);
        let same_way = fx * t.fwd.0 + fy * t.fwd.1;
        let own = n::get_entity_speed(veh);
        let chasing = dist < RAM_RANGE && ahead < RAM_AHEAD && same_way > RAM_SAME_WAY;
        // Coming the other way, ahead of the player and near its path.
        let oncoming = s.ram_oncoming && dist < ONCOMING_RANGE && ahead > 0.0 && beside.abs() < ONCOMING_SIDE && same_way < -RAM_SAME_WAY;
        let fast = t.speed >= RAM_MIN_SPEED;
        let head_on = match self.rams.get(&veh) {
            Some(&(since, head_on)) if now - since > RAM_TIME || !fast || !(if head_on { oncoming } else { chasing }) => {
                self.rams.remove(&veh);
                self.ram_after.insert(veh, now + RAM_COOLDOWN);
                return None;
            }
            Some(&(_, head_on)) => head_on,
            None => {
                let busy = self.rams.len() >= s.rammers || (oncoming && self.rams.values().any(|r| r.1));
                if !fast || !(chasing || oncoming) || own < RAM_MIN_SPEED || busy || self.ram_after.get(&veh).is_some_and(|&a| now < a) {
                    return None;
                }
                self.rams.insert(veh, (now, oncoming));
                let how = if oncoming { "head-on" } else if ahead < -2.0 { "from behind" } else { "from alongside" };
                crate::log(&format!("police: car {veh} goes for the player {how}, {dist:.0} m away"));
                oncoming
            }
        };
        // Where the player's car will be when it gets there; from behind, the
        // rear corner on its side (a PIT).
        let closing = if head_on { t.speed + own } else { s.ram + 5.0 };
        let lead = (dist / closing.max(5.0)).clamp(0.1, if head_on { 1.5 } else { 0.8 });
        let (mut ax, mut ay) = (t.pos.x + t.vel.x * lead, t.pos.y + t.vel.y * lead);
        let mut want = t.speed + s.ram;
        if head_on {
            // Head-on: no slowing down for it.
            want = own.max(t.speed * 0.5) + s.ram * 0.5;
        } else if ahead < -2.0 {
            let side = if beside >= 0.0 { 0.9 } else { -0.9 };
            ax += -t.fwd.0 * 1.6 + t.right.0 * side;
            ay += -t.fwd.1 * 1.6 + t.right.1 * side;
        } else {
            // Alongside: it slides in at about the player's speed.
            want = t.speed + 3.0;
        }
        let (ux, uy) = (ax - p.x, ay - p.y);
        let ul = ux.hypot(uy).max(1e-3);
        let dir = (ux / ul, uy / ul);
        if fx * dir.0 + fy * dir.1 < RAM_GIVE_UP {
            // Overshot, or it can't turn that far: give up.
            self.rams.remove(&veh);
            self.ram_after.insert(veh, now + RAM_COOLDOWN);
            return None;
        }
        Some((want.min(MAX_SPEED), dir))
    }

    /// A police car on its side or roof for RIGHT_AFTER, or hung up or
    /// jammed for STUCK_MS, is put back on the nearest road (keeping right of
    /// its centre line) facing the player, already rolling, as the player is
    /// after a wreck. `m`: its right, forward and up directions; `p`: where
    /// it is.
    fn unstick(&mut self, veh: i32, m: Axes, p: Vec3, player: Vec3, now: i32) {
        // Up pointing less than ~70 degrees from level: on its side or roof.
        let on_side = m.2[2] <= 0.35 && !n::is_entity_in_air(veh);
        if !on_side {
            self.flipped.remove(&veh);
        }
        let flipped = on_side && now - *self.flipped.entry(veh).or_insert(now) > RIGHT_AFTER;
        let why = if flipped {
            "on its side or roof"
        } else if n::is_vehicle_stuck_timer_up(veh, 2, STUCK_MS) {
            "hung up"
        } else if n::is_vehicle_stuck_timer_up(veh, 3, STUCK_MS) {
            "jammed"
        } else {
            return;
        };
        if self.moved.get(&veh).is_some_and(|&t| now - t < MOVE_AGAIN) {
            return;
        }
        let Some((node, h)) = n::closest_vehicle_node(p) else { return };
        if (node.x - player.x).hypot(node.y - player.y) < MOVE_CLEAR {
            return; // it would land on the player: later
        }
        // The road's direction that points towards the player.
        let r = h.to_radians();
        let h = if -r.sin() * (player.x - node.x) + r.cos() * (player.y - node.y) < 0.0 { (h + 180.0).rem_euclid(360.0) } else { h };
        let r = h.to_radians();
        let spot = Vec3::new(node.x + r.cos() * 2.0, node.y + r.sin() * 2.0, node.z);
        let ground = n::raycast(Vec3::new(spot.x, spot.y, spot.z + 5.0), Vec3::new(spot.x, spot.y, spot.z - 20.0), 1, veh).map_or(spot.z, |hit| hit.pos.z);
        n::set_entity_coords(veh, Vec3::new(spot.x, spot.y, ground + 0.5));
        n::set_entity_heading(veh, h);
        n::set_vehicle_on_ground_properly(veh);
        n::set_vehicle_forward_speed(veh, MOVE_SPEED);
        self.flipped.remove(&veh);
        self.motion.remove(&veh);
        self.moved.insert(veh, now);
        let far = (spot.x - p.x).hypot(spot.y - p.y);
        crate::log(&format!("police: car {veh} {why}, put back on the road {far:.0} m away, facing the player"));
    }

    /// Whether police car `veh` is going for the player's car.
    pub fn ramming(&self, veh: i32) -> bool {
        self.rams.contains_key(&veh)
    }

    /// `hands_off`: police cars takedown.rs has taken over.
    pub fn update(&mut self, s: &PoliceSettings, ped: i32, player_pos: Vec3, now: i32, hands_off: &dyn Fn(i32) -> bool) {
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
            self.pursue(s, ped, player_pos, now, hands_off);
        } else if !self.tuned.is_empty() {
            self.tuned.clear();
            self.may_leave.clear();
            self.assisted.clear();
            self.hit.clear();
            self.flipped.clear();
            self.motion.clear();
            self.moved.clear();
            self.rams.clear();
            self.ram_after.clear();
            for veh in self.armoured.drain() {
                if n::does_entity_exist(veh) {
                    n::set_entity_invincible(veh, false);
                }
            }
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

    /// `@police_grip`, `@police_accel` and `@police_brakes` for one police
    /// car on the ground, as one change to its velocity. `m`: its axes, `p`:
    /// where it is, `player`: the player's position and speed.
    #[allow(clippy::too_many_arguments)]
    fn control(&mut self, veh: i32, s: &PoliceSettings, dt: f32, m: Axes, p: Vec3, player: (Vec3, f32), ram: Ram, now: i32) {
        if dt <= 0.0 || n::is_entity_in_air(veh) || !n::is_vehicle_on_all_wheels(veh) {
            self.motion.remove(&veh);
            return;
        }
        let (r, f, u) = m;
        let v = n::get_entity_velocity(veh);
        let along = |d: [f32; 3]| d[0] * v.x + d[1] * v.y + d[2] * v.z;
        let (side, fwd0, up) = (along(r), along(f), along(u));
        let mut fwd = fwd0;
        // What GTA's physics did to its speed since last frame (the
        // driver's throttle or brakes, without this plugin's share),
        // smoothed; not known after a knock.
        let knocked = n::has_entity_collided_with_anything(veh);
        let accel = match self.motion.get(&veh) {
            Some(last) if !knocked && now - last.at < 250 => {
                let a = (fwd - last.fwd) / dt;
                (a.abs() < KNOCK).then_some(last.accel + (a - last.accel) * 0.5)
            }
            _ => None,
        };
        if let Some(a) = accel.filter(|_| fwd > 1.0 && ram.is_none()) {
            if a < -BRAKE_DETECT {
                // Braking: `brakes` times as hard, never into reverse.
                fwd = (fwd + (s.brakes - 1.0).max(0.0) * a * dt).max(0.0);
            } else if s.accel > 0.0 && a > -COAST {
                // On the throttle: up to `accel` g more...
                let mut push = if a > ACCEL_DETECT { s.accel * G * (a / FULL_THROTTLE).min(1.0) } else { 0.0 };
                let mut cap = MAX_SPEED;
                // ...and all of it while catching up with the player ahead.
                let (dx, dy) = (player.0.x - p.x, player.0.y - p.y);
                let dist = dx.hypot(dy);
                let flat = f[0].hypot(f[1]).max(1e-3);
                if dist > CATCHUP_MIN && (dx * f[0] + dy * f[1]) / (dist * flat) > CATCHUP_CONE {
                    let want = (player.1 + (dist / 10.0).min(CATCHUP_MARGIN)).min(MAX_SPEED);
                    if fwd < want {
                        push = s.accel * G;
                        cap = want;
                    }
                }
                if fwd < cap {
                    fwd = (fwd + push * dt).min(cap);
                }
            }
        }
        // Ramming: up to the speed it's going for, whatever its driver does.
        if let Some((want, _)) = ram {
            if fwd < want {
                fwd = (fwd + RAM_ACCEL * dt).min(want);
            }
        }
        // Grip: the same speed in the car's plane, turned towards where it
        // points; `grip` of the sideways slide goes each frame (at 60 fps).
        let gripping = s.grip > 0.0 && fwd >= GRIP_MIN_SPEED;
        let mut side2 = side;
        if gripping {
            side2 = side * (1.0 - s.grip.clamp(0.0, 1.0)).powf(dt * 60.0);
            fwd = (fwd * fwd + side * side - side2 * side2).sqrt();
        }
        if fwd != fwd0 || side2 != side {
            let nv: [f32; 3] = std::array::from_fn(|i| f[i] * fwd + r[i] * side2 + u[i] * up);
            n::set_entity_velocity(veh, nv[0], nv[1], nv[2]);
        }
        // Ramming: nose swung towards the target, as hard as RAM_MAX_G
        // allows at its speed.
        if let Some((_, dir)) = ram.filter(|_| fwd >= GRIP_MIN_SPEED) {
            let fl = f[0].hypot(f[1]).max(1e-3);
            let (fx, fy) = (f[0] / fl, f[1] / fl);
            let off = (fx * dir.1 - fy * dir.0).atan2(fx * dir.0 + fy * dir.1);
            let max_yaw = RAM_MAX_G * G / fwd;
            let w = n::get_entity_rotation_velocity(veh);
            n::set_entity_angular_velocity(veh, w.x, w.y, (off * RAM_TURN).clamp(-max_yaw, max_yaw));
        } else if gripping {
            // No spinning: turning held to what GRIP_MAX_G allows at its
            // speed.
            let w = n::get_entity_rotation_velocity(veh);
            let max_yaw = GRIP_MAX_G * G / fwd;
            if w.z.abs() > max_yaw {
                n::set_entity_angular_velocity(veh, w.x, w.y, w.z.clamp(-max_yaw, max_yaw));
            }
        }
        self.motion.insert(veh, Motion { fwd, accel: accel.unwrap_or(0.0), at: now });
    }
}
