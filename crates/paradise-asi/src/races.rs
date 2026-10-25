//! Races, Burnout style. They're placed in the viewer's Races panel and
//! installed as `paradise_races.json` (game folder): each has a start (and
//! the way the cars face there), optional checkpoints in order and a
//! finish. There's no route: the rivals find their own way, the player
//! has the GPS.
//!
//! - Every complete race has a map blip at its start (nothing in the
//!   world until it starts). Points clicked on something above the road (a
//!   footbridge) go down to the road under them (the road nearest within
//!   SNAP m, heights counting double).
//! - Driving a car, nearly stopped in a start (within START_RADIUS m),
//!   with accelerate and brake both held for `@race_hold` s, the race begins: the
//!   player's car goes onto the start, `@race_opponents` rivals line up
//!   beside and behind it, 3-2-1, go. Like Burnout, the rivals drive a mix
//!   of cars like the player's, a little better: picked at random from the
//!   CHOICES road cars (the junkyard's list, shops.rs) at least as fast and
//!   quick off the line as the player's car (stock), nearest to BETTER
//!   times its top speed, acceleration and traction (bikes for a bike);
//!   only if there are none, the nearest whatever they are.
//! - Rivals drive to each checkpoint in turn, Burnout style (rival.rs:
//!   any road either way, corner speeds, boost, put back on the road after
//!   a wreck). The player gets a checkpoint
//!   marker, a blip and the GPS route (gps.rs) to the next one, and the
//!   position, checkpoints and time on screen (the clock stops while the
//!   pause menu is open).
//! - Catch-up: with the player 1st or 2nd, the last rival more than
//!   CATCH_UP_GAP m behind them (straight lines between the race's points)
//!   is moved on along its route to about CATCH_UP_TO m behind, out of the
//!   player's sight (rival.rs), every CATCH_UP_MS at most.
//! - No takedowns during a race (lib.rs): rear-ending a rival would blow it
//!   up, and pitting one would set one off.
//! - No police: a race can't be started while wanted, and the player can't
//!   be wanted during one (GTA's max wanted level is 0 until it's over).
//! - Traffic: cleared within START_CLEAR m of the start for the line-up,
//!   and `@race_traffic` times normal until the race is over (lib.rs).
//! - The finish shows the result; the rivals are then left to the game.
//!   Out of a car for OUT_OF_CAR_MS, dead, or a character switch cancels
//!   the race.
//! - Logged: the races loaded (and any left out or points moved down),
//!   each start, the rivals made (or why a place was skipped), every
//!   checkpoint (the player's and the rivals'; rival.rs logs their
//!   driving), the result or why the race was cancelled.

use crate::gps::GpsWatch;
use crate::natives as n;
use crate::boost::BoostSettings;
use crate::propshield::{joaat, PropShield};
use crate::rival::{self, Rival};
use crate::shv::Vec3;

const FILE: &str = "paradise_races.json";
const START_RADIUS: f32 = 10.0;
/// A race point goes down to a road this close (m, flat) under it...
const SNAP: f32 = 30.0;
/// ...if it's more than this (m) higher or lower.
const SNAP_HEIGHT: f32 = 2.0;
/// At most this fast (m/s) to start a race.
const START_SPEED: f32 = 5.0;
const COUNTDOWN_MS: i32 = 3000;
/// Models not loaded by then: the race starts without rivals.
const LOAD_MS: i32 = 5000;
const CONTROL_ACCELERATE: i32 = 71;
const CONTROL_BRAKE: i32 = 72;
/// Checkpoint size on screen, and how close (m, flat) counts as through it.
const CP_DIAMETER: f32 = 20.0;
const PLAYER_REACH: f32 = 12.0;
pub const RIVAL_REACH: f32 = 20.0;
/// Height (m) a checkpoint counts over, up and down.
pub const REACH_HEIGHT: f32 = 10.0;
/// Rivals' driver model; their driving is looked at every RIVAL_MS.
const RIVAL_DRIVER: &str = "a_m_y_motox_01";
const RIVAL_MS: i32 = 250;
const OUT_OF_CAR_MS: i32 = 10_000;
const RESULT_MS: i32 = 8000;
/// Rival places beside and behind the start (m right, m ahead).
const GRID: [(f32, f32); 9] = [(4.5, 0.0), (-4.5, 0.0), (0.0, -8.0), (4.5, -8.0), (-4.5, -8.0), (0.0, -16.0), (4.5, -16.0), (-4.5, -16.0), (0.0, -24.0)];
const MAX_RIVALS: usize = 8;
/// Traffic within this (m) of the start is cleared for the line-up.
const START_CLEAR: f32 = 60.0;
const CATCH_UP_GAP: f32 = 350.0;
const CATCH_UP_TO: f32 = 150.0;
const CATCH_UP_MS: i32 = 2000;
/// Rival cars are picked from this many cars nearest the player's.
const CHOICES: usize = 10;
/// Rival cars are nearest this much better than the player's (times).
const BETTER: f32 = 1.05;
/// Classes rivals' cars may be from (GET_VEHICLE_CLASS): compacts, sedans,
/// SUVs, coupes, muscle, sports classics, sports, super, off-road, vans,
/// open wheel; and the player's own class (motorcycles only for a bike).
const RIVAL_CLASSES: [i32; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 12, 22];
const MOTORCYCLES: i32 = 8;
/// Blips: race start (sprite, colour), checkpoint, finish.
const START_BLIP: i32 = 315;
const CHECKPOINT_BLIP: (i32, i32) = (1, 5);
const FINISH_BLIP: i32 = 38;
const CHECKPOINT_RGBA: [i32; 4] = [255, 200, 40, 150];
const SOUNDS: &str = "HUD_MINI_GAME_SOUNDSET";

struct Race {
    name: String,
    start: Vec3,
    heading: f32,
    /// The checkpoints, then the finish.
    points: Vec<Vec3>,
}

impl Race {
    fn forward(&self) -> (f32, f32) {
        let h = self.heading.to_radians();
        (-h.sin(), h.cos())
    }
}

enum Phase {
    /// Models loading (since).
    Loading(i32),
    /// 3-2-1 (since, seconds announced).
    Countdown(i32, i32),
    Racing,
    /// Result on screen (since, text).
    Result(i32, String),
}

struct Run {
    race: usize,
    ped: i32,
    car: i32,
    phase: Phase,
    /// Race time (ms), counted only while the pause menu is closed.
    clock: i32,
    last_now: i32,
    next: usize,
    checkpoint: i32,
    blip: i32,
    rivals: Vec<Rival>,
    out_since: Option<i32>,
    last_rivals: i32,
    last_catch_up: i32,
    /// GTA's max wanted level before the race (put back after).
    max_wanted: i32,
    /// Each rival's car model, in grid order.
    rival_models: Vec<u32>,
    driver_model: u32,
}

/// Races' ini settings.
#[derive(Clone, Copy)]
pub struct RaceSettings {
    /// Rivals in a race (`@race_opponents`).
    pub opponents: usize,
    /// How long (ms) accelerate and brake are held to start (`@race_hold`).
    pub hold_ms: i32,
    /// Rivals' speed limits, times (`@race_rival_speed`).
    pub rival_speed: f32,
}

impl Default for RaceSettings {
    fn default() -> Self {
        Self { opponents: 3, hold_ms: 2500, rival_speed: 1.1 }
    }
}

#[derive(Default)]
pub struct Races {
    settings: RaceSettings,
    races: Vec<Race>,
    loaded: bool,
    blips: Vec<i32>,
    /// Cars rivals may drive (loaded at the first race).
    cars: Option<Vec<crate::shops::Car>>,
    /// Race being held to start, and since when.
    hold: Option<(usize, i32)>,
    /// Race whose start the player is in while wanted (logged once).
    refused: Option<usize>,
    run: Option<Run>,
    /// Text on screen until a time (cancelled races).
    message: Option<(String, i32)>,
}

fn flat(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn reached(p: Vec3, target: Vec3, reach: f32) -> bool {
    flat(p, target) <= reach && (p.z - target.z).abs() <= REACH_HEIGHT
}

fn ordinal(k: usize) -> String {
    let suffix = match (k % 10, k % 100) {
        (1, x) if x != 11 => "st",
        (2, x) if x != 12 => "nd",
        (3, x) if x != 13 => "rd",
        _ => "th",
    };
    format!("{k}{suffix}")
}

fn time(ms: i32) -> String {
    format!("{}:{:02}.{}", ms / 60_000, ms / 1000 % 60, ms / 100 % 10)
}

impl Races {
    fn load(&mut self, gps: &mut GpsWatch) {
        self.loaded = true;
        let Ok(text) = std::fs::read_to_string(FILE) else {
            crate::log(&format!("races: no {FILE}; no races"));
            return;
        };
        let Some(doc) = json::parse(&text) else {
            crate::log(&format!("races: {FILE} isn't valid JSON; no races"));
            return;
        };
        let point = |v: Option<&json::Value>| -> Option<Vec3> {
            let a = v?.array()?;
            (a.len() == 3).then(|| Vec3::new(a[0].num().unwrap_or(0.0) as f32, a[1].num().unwrap_or(0.0) as f32, a[2].num().unwrap_or(0.0) as f32))
        };
        for (k, r) in doc.get("races").and_then(json::Value::array).into_iter().flatten().enumerate() {
            let name = r.get("name").and_then(json::Value::str).map(str::to_string).unwrap_or_else(|| format!("Race {}", k + 1));
            let (Some(start), Some(finish)) = (point(r.get("start")), point(r.get("finish"))) else {
                crate::log(&format!("races: '{name}' left out: it has no start or no finish yet"));
                continue;
            };
            let mut start = start;
            let mut points: Vec<Vec3> = r.get("checkpoints").and_then(json::Value::array).into_iter().flatten().filter_map(|c| point(Some(c))).collect();
            points.push(finish);
            let last = points.len();
            for (k, p) in std::iter::once(&mut start).chain(points.iter_mut()).enumerate() {
                if let Some(z) = gps.road_height(*p, SNAP).filter(|z| (z - p.z).abs() > SNAP_HEIGHT) {
                    let what = match k {
                        0 => "start".to_string(),
                        k if k == last => "finish".to_string(),
                        k => format!("checkpoint {k}"),
                    };
                    crate::log(&format!("races: '{name}' {what} at ({:.0}, {:.0}) moved from height {:.1} to the road at {z:.1}", p.x, p.y, p.z));
                    p.z = z;
                }
            }
            let finish = points[last - 1];
            let heading = r.get("heading").and_then(json::Value::num).unwrap_or(0.0) as f32;
            crate::log(&format!(
                "races: '{name}' from ({:.0}, {:.0}, {:.0}) heading {heading:.0}, {} checkpoint(s), finish at ({:.0}, {:.0}, {:.0})",
                start.x,
                start.y,
                start.z,
                points.len() - 1,
                finish.x,
                finish.y,
                finish.z
            ));
            self.races.push(Race { name, start, heading, points });
        }
        for r in &self.races {
            let b = n::add_blip_for_coord(r.start);
            n::set_blip_sprite(b, START_BLIP);
            n::set_blip_as_short_range(b, true);
            n::set_blip_name(b, &format!("Race: {}", r.name));
            self.blips.push(b);
        }
        crate::log(&format!("races: {} race(s) from {FILE}", self.races.len()));
    }

    /// The plugin's own blips (kept by missions.rs).
    pub fn blips(&self) -> Vec<i32> {
        let run = self.run.iter().flat_map(|r| std::iter::once(r.blip).chain(r.rivals.iter().map(Rival::blip)));
        self.blips.iter().copied().chain(run.filter(|&b| b != 0)).collect()
    }

    /// A race on (from lining up to the result): no takedowns.
    pub fn in_race(&self) -> bool {
        self.run.is_some()
    }

    /// Lining up or counting down: no crashes or wrecks meanwhile (the car
    /// is held still while the player may be revving).
    pub fn holding(&self) -> bool {
        self.run.as_ref().is_some_and(|r| matches!(r.phase, Phase::Loading(_) | Phase::Countdown(..)))
    }

    /// `driven`: the car the player drives (0: none); `boost`: the
    /// player's boost settings, which the rivals' boost goes by.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, s: &RaceSettings, boost: &BoostSettings, props: &mut PropShield, ped: i32, driven: i32, gps: &mut GpsWatch, now: i32) {
        let opponents = s.opponents;
        self.settings = *s;
        if !self.loaded {
            self.load(gps);
        }
        if let Some((text, until)) = &self.message {
            if now < *until {
                n::draw_text_centred(text, 0.5, 0.30, 0.9, [255, 80, 60, 255]);
            } else {
                self.message = None;
            }
        }
        if self.races.is_empty() {
            return;
        }
        let pos = n::get_entity_coords(if driven != 0 { driven } else { ped });
        if self.run.is_some() {
            self.race(opponents, boost, props, ped, driven, pos, gps, now);
        } else {
            self.idle(opponents, s.hold_ms, ped, driven, pos, now);
        }
    }

    /// Not racing: holding to start (for `hold_ms`).
    fn idle(&mut self, opponents: usize, hold_ms: i32, ped: i32, driven: i32, pos: Vec3, now: i32) {
        let here = (driven != 0)
            .then(|| self.races.iter().position(|r| reached(pos, r.start, START_RADIUS)))
            .flatten();
        let Some(k) = here else {
            self.hold = None;
            self.refused = None;
            return;
        };
        let name = &self.races[k].name;
        let wanted = n::get_player_wanted_level(n::player_id());
        if wanted > 0 {
            n::draw_text_centred(&format!("{name}: lose the police to start"), 0.5, 0.80, 0.6, [255, 80, 60, 255]);
            if self.refused != Some(k) {
                self.refused = Some(k);
                crate::log(&format!("races: '{name}' can't start: the player is wanted (level {wanted})"));
            }
            self.hold = None;
            return;
        }
        self.refused = None;
        if n::get_entity_speed(driven) > START_SPEED {
            n::draw_text_centred(&format!("{name}: stop here to start"), 0.5, 0.80, 0.6, [255, 200, 40, 255]);
            self.hold = None;
            return;
        }
        let both = n::is_disabled_control_pressed(CONTROL_ACCELERATE) && n::is_disabled_control_pressed(CONTROL_BRAKE);
        if !both {
            n::draw_text_centred(&format!("{name}: hold accelerate and brake to start"), 0.5, 0.80, 0.6, [255, 200, 40, 255]);
            self.hold = None;
            return;
        }
        let since = match self.hold {
            Some((h, t)) if h == k => t,
            _ => {
                self.hold = Some((k, now));
                now
            }
        };
        let done = ((now - since) as f32 / hold_ms as f32).min(1.0);
        n::draw_text_centred(&format!("{name}: starting..."), 0.5, 0.80, 0.6, [255, 200, 40, 255]);
        n::draw_rect(0.5, 0.86, 0.2, 0.012, [0, 0, 0, 160]);
        n::draw_rect(0.4 + 0.1 * done, 0.86, 0.2 * done, 0.012, [255, 200, 40, 255]);
        if done >= 1.0 {
            self.hold = None;
            self.begin(k, opponents, hold_ms, ped, driven, now);
        }
    }

    /// Onto the start; models requested for the rivals.
    fn begin(&mut self, k: usize, opponents: usize, hold_ms: i32, ped: i32, car: i32, now: i32) {
        let r = &self.races[k];
        n::set_entity_coords(car, Vec3::new(r.start.x, r.start.y, r.start.z + 0.5));
        n::set_entity_heading(car, r.heading);
        n::set_vehicle_on_ground_properly(car);
        n::set_entity_velocity(car, 0.0, 0.0, 0.0);
        n::freeze_entity_position(car, true);
        let driver_model = joaat(RIVAL_DRIVER);
        let max_wanted = n::get_max_wanted_level();
        n::set_max_wanted_level(0);
        n::clear_player_wanted_level(n::player_id());
        crate::log(&format!("races: '{}' started (accelerate and brake held {:.1} s): {opponents} rival(s) wanted", r.name, hold_ms as f32 / 1000.0));
        let rival_models = if opponents > 0 { self.pick_cars(n::get_entity_model(car), opponents.min(MAX_RIVALS)) } else { Vec::new() };
        for &m in rival_models.iter().chain(std::iter::once(&driver_model).filter(|_| opponents > 0)) {
            n::request_model(m);
        }
        self.run = Some(Run {
            race: k,
            ped,
            car,
            phase: Phase::Loading(now),
            clock: 0,
            last_now: now,
            next: 0,
            checkpoint: 0,
            blip: 0,
            rivals: Vec::new(),
            out_since: None,
            last_rivals: now,
            last_catch_up: now,
            max_wanted,
            rival_models,
            driver_model,
        });
        if opponents == 0 {
            self.line_up(0, now);
        }
    }

    /// The rivals on the grid (as many places as are clear), then the
    /// countdown.
    fn line_up(&mut self, opponents: usize, now: i32) {
        let run = self.run.as_mut().unwrap();
        let r = &self.races[run.race];
        let (fx, fy) = r.forward();
        let (rx, ry) = (fy, -fx);
        if opponents > 0 {
            n::clear_area_of_vehicles(r.start, START_CLEAR);
            crate::log(&format!("races: traffic cleared within {START_CLEAR:.0} m of the start"));
        }
        let eye = Vec3::new(r.start.x, r.start.y, r.start.z + 1.0);
        for &(right, ahead) in &GRID {
            if run.rivals.len() >= opponents.min(MAX_RIVALS) {
                break;
            }
            let (x, y) = (r.start.x + rx * right + fx * ahead, r.start.y + ry * right + fy * ahead);
            let Some(z) = n::get_ground_z(Vec3::new(x, y, r.start.z + 5.0)).filter(|z| (z - r.start.z).abs() <= 3.0) else {
                crate::log(&format!("races: no rival at ({right:+.1} m right, {ahead:+.1} m ahead): no ground level with the start"));
                continue;
            };
            let at = Vec3::new(x, y, z + 1.0);
            if let Some(hit) = n::raycast(eye, at, 3, run.car) {
                crate::log(&format!(
                    "races: no rival at ({right:+.1} m right, {ahead:+.1} m ahead): blocked at ({:.1}, {:.1}, {:.1})",
                    hit.pos.x,
                    hit.pos.y,
                    hit.pos.z
                ));
                continue;
            }
            let model = run.rival_models[run.rivals.len() % run.rival_models.len()];
            let car = n::create_vehicle(model, Vec3::new(x, y, z + 0.5), r.heading);
            if car == 0 {
                crate::log("races: a rival's car couldn't be made");
                continue;
            }
            n::set_entity_as_mission_entity(car);
            n::set_vehicle_on_ground_properly(car);
            n::freeze_entity_position(car, true);
            let ped = n::create_ped_inside_vehicle(car, 4, run.driver_model, -1);
            if ped == 0 {
                crate::log("races: a rival's driver couldn't be made");
                n::delete_vehicle(car);
                continue;
            }
            Rival::set_up_driver(ped);
            n::set_vehicle_engine_on(car);
            crate::log(&format!("races: rival {} at ({right:+.1} m right, {ahead:+.1} m ahead), car {car}, driver {ped}", run.rivals.len() + 1));
            let mut v = Rival::new(car, ped, model);
            v.speed = self.settings.rival_speed;
            v.mark(run.rivals.len());
            run.rivals.push(v);
        }
        // (The models stay loaded for rivals put back after a wreck.)
        if opponents > 0 {
            crate::log(&format!("races: {} of {opponents} rival(s) lined up, speed limits x{:.2}", run.rivals.len(), self.settings.rival_speed));
        }
        run.phase = Phase::Countdown(now, 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn race(&mut self, opponents: usize, boost: &BoostSettings, props: &mut PropShield, ped: i32, driven: i32, pos: Vec3, gps: &mut GpsWatch, now: i32) {
        let run = self.run.as_mut().unwrap();
        let paused = n::is_pause_menu_active();
        if matches!(run.phase, Phase::Racing) && !paused {
            run.clock += now - run.last_now;
        }
        run.last_now = now;
        // Never wanted during a race (in case anything sets it anyway).
        if n::get_player_wanted_level(n::player_id()) > 0 {
            n::clear_player_wanted_level(n::player_id());
        }
        // Cancelled?
        let why = if ped != run.ped {
            Some("the player changed character")
        } else if n::is_entity_dead(ped) {
            Some("the player died")
        } else if driven == 0 && !matches!(run.phase, Phase::Result(..)) {
            let since = *run.out_since.get_or_insert(now);
            (now - since >= OUT_OF_CAR_MS || !matches!(run.phase, Phase::Racing)).then_some("the player left the car")
        } else {
            run.out_since = None;
            None
        };
        if let Some(why) = why {
            let name = self.races[run.race].name.clone();
            crate::log(&format!("races: '{name}' cancelled: {why} ({} in)", time(run.clock)));
            self.message = Some((format!("{name}: race cancelled"), now + 3000));
            self.end(gps);
            return;
        }
        match run.phase {
            Phase::Loading(since) => {
                let ready = run.rival_models.iter().all(|&m| n::has_model_loaded(m)) && n::has_model_loaded(run.driver_model);
                if ready || now - since >= LOAD_MS {
                    if !ready {
                        crate::log(&format!("races: rival models not loaded after {} s; racing alone", LOAD_MS / 1000));
                    }
                    self.line_up(if ready { opponents } else { 0 }, now);
                }
            }
            Phase::Countdown(since, said) => {
                // Whole seconds left, rounded up.
                let left = (COUNTDOWN_MS - (now - since) + 999) / 1000;
                if left > 0 {
                    if said != left {
                        run.phase = Phase::Countdown(since, left);
                        n::play_sound_frontend("3_2_1", SOUNDS);
                    }
                    n::draw_text_centred(&left.to_string(), 0.5, 0.30, 2.0, [255, 200, 40, 255]);
                } else {
                    n::play_sound_frontend("GO", SOUNDS);
                    n::freeze_entity_position(run.car, false);
                    let first = self.races[run.race].points[0];
                    for (k, v) in run.rivals.iter_mut().enumerate() {
                        n::freeze_entity_position(v.car, false);
                        v.plan(k, first, gps);
                        v.launch(k);
                    }
                    // Rivals steered straight away.
                    run.last_rivals = now - RIVAL_MS;
                    run.phase = Phase::Racing;
                    run.clock = 0;
                    crate::log(&format!("races: '{}' go, {} rival(s)", self.races[run.race].name, run.rivals.len()));
                    self.mark_next(gps);
                }
            }
            Phase::Racing => self.racing(boost, props, pos, gps, now),
            Phase::Result(since, ref text) => {
                if now - since < RESULT_MS {
                    n::draw_text_centred(text, 0.5, 0.30, 1.2, [255, 200, 40, 255]);
                } else {
                    self.end(gps);
                }
            }
        }
    }

    fn racing(&mut self, boost: &BoostSettings, props: &mut PropShield, pos: Vec3, gps: &mut GpsWatch, now: i32) {
        let run = self.run.as_mut().unwrap();
        let r = &self.races[run.race];
        if run.clock < 1000 {
            n::draw_text_centred("GO!", 0.5, 0.30, 2.0, [80, 255, 80, 255]);
        }
        // Rivals.
        let dt = n::get_frame_time().clamp(0.0, 0.1);
        for (k, v) in run.rivals.iter_mut().enumerate() {
            v.frame(k, boost, props, run.car, dt, now);
        }
        if now - run.last_rivals >= RIVAL_MS {
            run.last_rivals = now;
            for (k, v) in run.rivals.iter_mut().enumerate() {
                match v.update(k, &r.points, run.clock, pos, run.driver_model, gps, now) {
                    Some(rival::Event::Finished) => {
                        crate::log(&format!("races: rival {} finished in {} ({} wreck(s))", k + 1, time(run.clock), v.wrecks));
                        v.finish();
                        v.wander();
                    }
                    Some(rival::Event::Checkpoint) => crate::log(&format!("races: rival {} through checkpoint {} at {}", k + 1, v.next, time(run.clock))),
                    None => {}
                }
            }
        }
        // Position: rivals done, further along, or as far and closer.
        let mine = flat(pos, r.points[run.next]);
        let racers = 1 + run.rivals.len();
        let place = 1 + run
            .rivals
            .iter()
            .filter(|v| {
                v.done.is_some()
                    || v.next > run.next
                    || (v.next == run.next && n::does_entity_exist(v.car) && flat(n::get_entity_coords(v.car), r.points[run.next]) < mine)
            })
            .count();
        // Catch-up for the last rival when the player's leading.
        if place <= 2 && now - run.last_catch_up >= CATCH_UP_MS {
            run.last_catch_up = now;
            // Metres left: to the next point, then point to point.
            let left = |at: Vec3, next: usize| flat(at, r.points[next]) + r.points[next..].windows(2).map(|w| flat(w[0], w[1])).sum::<f32>();
            let mine = left(pos, run.next);
            let last = run
                .rivals
                .iter()
                .enumerate()
                .filter(|(_, v)| v.done.is_none() && !v.wrecked() && n::does_entity_exist(v.car))
                .map(|(k, v)| (k, left(n::get_entity_coords(v.car), v.next)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((k, theirs)) = last {
                let behind = theirs - mine;
                if behind > CATCH_UP_GAP && run.rivals.iter().filter(|v| v.done.is_none()).count() > 0 {
                    crate::log(&format!("races: rival {} last, {behind:.0} m behind the player ({}): catch-up", k + 1, ordinal(place)));
                    if run.rivals[k].move_up(k, behind - CATCH_UP_TO, pos, now).is_none() {
                        crate::log(&format!("races: rival {} can't be moved up now (too near the player, wrecked, or at its checkpoint)", k + 1));
                    }
                }
            }
        }
        let checkpoints = r.points.len() - 1;
        let progress = if checkpoints > 0 { format!("   checkpoint {}/{checkpoints}", run.next.min(checkpoints)) } else { String::new() };
        n::draw_text_centred(&format!("{}   {} of {racers}{progress}   {}", r.name, ordinal(place), time(run.clock)), 0.5, 0.03, 0.6, [255, 255, 255, 255]);
        // The player.
        if !reached(pos, r.points[run.next], PLAYER_REACH) {
            return;
        }
        run.next += 1;
        if run.next < r.points.len() {
            n::play_sound_frontend("CHECKPOINT_NORMAL", SOUNDS);
            crate::log(&format!("races: through checkpoint {} of {checkpoints} at {}, {} of {racers}", run.next, time(run.clock), ordinal(place)));
            self.mark_next(gps);
            return;
        }
        n::play_sound_frontend("CHECKPOINT_PERFECT", SOUNDS);
        let text = format!("{} place   {}", ordinal(place), time(run.clock));
        crate::log(&format!("races: '{}' finished in {}, {} of {racers}", r.name, time(run.clock), ordinal(place)));
        for (k, v) in run.rivals.iter().enumerate() {
            let state = match v.done {
                Some(t) => format!("finished in {}", time(t)),
                None => format!("heading for point {} of {}", v.next + 1, r.points.len()),
            };
            crate::log(&format!("races:   rival {}: {state}, {} wreck(s)", k + 1, v.wrecks));
        }
        run.phase = Phase::Result(now, text);
        Self::unmark(run);
        gps.set_target(None);
    }

    /// The player's next point: its checkpoint, blip and GPS route.
    fn mark_next(&mut self, gps: &mut GpsWatch) {
        let run = self.run.as_mut().unwrap();
        let r = &self.races[run.race];
        Self::unmark(run);
        let at = r.points[run.next];
        let last = run.next + 1 == r.points.len();
        let after = r.points.get(run.next + 1).copied().unwrap_or(at);
        run.checkpoint = n::create_checkpoint(if last { 4 } else { 0 }, at, after, CP_DIAMETER, CHECKPOINT_RGBA);
        run.blip = n::add_blip_for_coord(at);
        if last {
            n::set_blip_sprite(run.blip, FINISH_BLIP);
            n::set_blip_name(run.blip, "Finish");
        } else {
            n::set_blip_sprite(run.blip, CHECKPOINT_BLIP.0);
            n::set_blip_colour(run.blip, CHECKPOINT_BLIP.1);
            n::set_blip_name(run.blip, &format!("Checkpoint {}", run.next + 1));
        }
        gps.set_target(Some(at));
    }

    fn unmark(run: &mut Run) {
        if run.checkpoint != 0 {
            n::delete_checkpoint(run.checkpoint);
            run.checkpoint = 0;
        }
        if run.blip != 0 {
            n::remove_blip(run.blip);
            run.blip = 0;
        }
    }

    /// Race over: markers gone, the player's car free, rivals left to the
    /// game (driving off like traffic).
    fn end(&mut self, gps: &mut GpsWatch) {
        let Some(mut run) = self.run.take() else { return };
        n::set_max_wanted_level(if run.max_wanted > 0 { run.max_wanted } else { 5 });
        Self::unmark(&mut run);
        gps.set_target(None);
        if n::does_entity_exist(run.car) {
            n::freeze_entity_position(run.car, false);
        }
        for v in &mut run.rivals {
            v.unmark();
            v.finish();
            if n::does_entity_exist(v.car) {
                n::freeze_entity_position(v.car, false);
            }
            if n::does_entity_exist(v.ped) && !n::is_entity_dead(v.ped) && v.done.is_none() {
                v.wander();
            }
            n::set_entity_as_no_longer_needed(v.ped);
            n::set_entity_as_no_longer_needed(v.car);
        }
        for &m in run.rival_models.iter().chain(std::iter::once(&run.driver_model)) {
            n::set_model_as_no_longer_needed(m);
        }
    }

    /// `count` rival car models like the player's (`player_model`), a
    /// little better: at random from the CHOICES at least its stock top
    /// speed and acceleration, nearest BETTER times its top speed (most),
    /// acceleration and traction, other models first.
    fn pick_cars(&mut self, player_model: u32, count: usize) -> Vec<u32> {
        let cars = self.cars.get_or_insert_with(crate::shops::load_cars);
        let mine = n::model_performance(player_model);
        let class = n::vehicle_class_from_name(player_model);
        let allowed = |c: i32| if class == MOTORCYCLES { c == MOTORCYCLES } else { c == class || RIVAL_CLASSES.contains(&c) };
        let rel = |a: f32, b: f32| if b > 0.0 { (a - b).abs() / b } else { 0.0 };
        let score = |c: &crate::shops::Car| rel(c.perf[0], mine[0] * BETTER) + 0.6 * rel(c.perf[1], mine[1] * BETTER) + 0.3 * rel(c.perf[3], mine[3] * BETTER);
        let usable = |c: &&crate::shops::Car| allowed(c.class) && c.perf[0] > 0.0 && c.perf[1] > 0.0;
        // At least as fast and as quick off the line (a hair under counts:
        // the same car in another body).
        let as_good = |c: &&crate::shops::Car| c.perf[0] >= mine[0] * 0.995 && c.perf[1] >= mine[1] * 0.995;
        let mut near: Vec<(f32, &crate::shops::Car)> = cars.iter().filter(usable).filter(as_good).map(|c| (score(c), c)).collect();
        let better = !near.iter().all(|(_, c)| c.hash == player_model);
        if !better {
            crate::log("races: no car at least as fast and quick as the player's; rivals get the nearest instead");
            near = cars.iter().filter(usable).map(|c| (score(c), c)).collect();
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut choices: Vec<&(f32, &crate::shops::Car)> = near.iter().filter(|(_, c)| c.hash != player_model).take(CHOICES).collect();
        let you = n::display_name(player_model).unwrap_or_else(|| format!("{player_model:#x}"));
        if choices.is_empty() {
            crate::log(&format!("races: no cars like the player's {you} to pick from; rivals drive the same"));
            return vec![player_model; count];
        }
        crate::log(&format!(
            "races: rival cars {} the player's {you} (top {:.1} m/s, acceleration {:.2}, traction {:.2}) picked from: {}",
            if better { "at least as good as" } else { "nearest" },
            mine[0],
            mine[1],
            mine[3],
            choices.iter().map(|(d, c)| format!("{} ({d:.2})", c.name)).collect::<Vec<_>>().join(", ")
        ));
        let mut picked = Vec::new();
        for k in 0..count {
            // Each car once, until there aren't enough.
            if choices.is_empty() {
                choices = near.iter().filter(|(_, c)| c.hash != player_model).take(CHOICES).collect();
            }
            let (d, car) = choices.remove(n::random_int(0, choices.len() as i32).clamp(0, choices.len() as i32 - 1) as usize);
            crate::log(&format!(
                "races: rival {} drives a {} (top {:.1} m/s {:+.0}%, acceleration {:.2} {:+.0}%, traction {:.2}; {d:.2} off)",
                k + 1,
                car.name,
                car.perf[0],
                (car.perf[0] / mine[0] - 1.0) * 100.0,
                car.perf[1],
                (car.perf[1] / mine[1].max(1e-3) - 1.0) * 100.0,
                car.perf[3]
            ));
            picked.push(car.hash);
        }
        picked
    }
}

/// Just enough JSON for the races file.
mod json {
    #[allow(dead_code)]
    pub enum Value {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn array(&self) -> Option<&Vec<Value>> {
            if let Value::Arr(a) = self { Some(a) } else { None }
        }
        pub fn num(&self) -> Option<f64> {
            if let Value::Num(x) = self { Some(*x) } else { None }
        }
        pub fn str(&self) -> Option<&str> {
            if let Value::Str(s) = self { Some(s) } else { None }
        }
    }

    pub fn parse(text: &str) -> Option<Value> {
        let mut p = Parser { s: text.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        (p.i == p.s.len()).then_some(v)
    }

    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn ws(&mut self) {
            while self.s.get(self.i).is_some_and(u8::is_ascii_whitespace) {
                self.i += 1;
            }
        }

        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            let ok = self.s.get(self.i) == Some(&c);
            if ok {
                self.i += 1;
            }
            ok
        }

        fn word(&mut self, w: &str, v: Value) -> Option<Value> {
            self.s[self.i..].starts_with(w.as_bytes()).then(|| {
                self.i += w.len();
                v
            })
        }

        fn value(&mut self) -> Option<Value> {
            self.ws();
            match *self.s.get(self.i)? {
                b'{' => {
                    self.i += 1;
                    let mut o = Vec::new();
                    if self.eat(b'}') {
                        return Some(Value::Obj(o));
                    }
                    loop {
                        self.ws();
                        let Value::Str(k) = self.string()? else { return None };
                        if !self.eat(b':') {
                            return None;
                        }
                        o.push((k, self.value()?));
                        if self.eat(b'}') {
                            return Some(Value::Obj(o));
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                b'[' => {
                    self.i += 1;
                    let mut a = Vec::new();
                    if self.eat(b']') {
                        return Some(Value::Arr(a));
                    }
                    loop {
                        a.push(self.value()?);
                        if self.eat(b']') {
                            return Some(Value::Arr(a));
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                b'"' => self.string(),
                b't' => self.word("true", Value::Bool(true)),
                b'f' => self.word("false", Value::Bool(false)),
                b'n' => self.word("null", Value::Null),
                _ => {
                    let start = self.i;
                    while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || b"+-.eE".contains(c)) {
                        self.i += 1;
                    }
                    std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok().map(Value::Num)
                }
            }
        }

        fn string(&mut self) -> Option<Value> {
            if self.s.get(self.i) != Some(&b'"') {
                return None;
            }
            self.i += 1;
            let mut out = Vec::new();
            loop {
                match *self.s.get(self.i)? {
                    b'"' => {
                        self.i += 1;
                        return Some(Value::Str(String::from_utf8_lossy(&out).into_owned()));
                    }
                    b'\\' => {
                        let c = *self.s.get(self.i + 1)?;
                        self.i += 2;
                        let ch = match c {
                            b'n' => '\n',
                            b't' => '\t',
                            b'r' => '\r',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'u' => {
                                let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                                self.i += 4;
                                char::from_u32(u32::from_str_radix(hex, 16).ok()?).unwrap_or('\u{fffd}')
                            }
                            other => other as char,
                        };
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    c => {
                        out.push(c);
                        self.i += 1;
                    }
                }
            }
        }
    }
}
