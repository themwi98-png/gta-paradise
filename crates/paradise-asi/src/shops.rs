//! Burnout's drive-thrus (`paradise_shops.txt`, from Burnout's own trigger
//! boxes; written by bpx gta-shops). Each gets a map blip. Driving into one:
//!
//! - auto repair: the car is fixed (bodywork, engine, tyres) and washed;
//! - gas station: the boost gauge is filled;
//! - paint shop: the car is resprayed in random colours, as in Burnout;
//! - junkyard: the car stops dead and a menu opens (not while wanted): modify
//!   the car, like Los Santos Customs but free (respray, wheels, body and
//!   performance parts, window tint, plate, livery, turbo, xenons), or
//!   swap it for any road vehicle in the game, by class (the list is
//!   `paradise_cars.txt`, every road vehicle's model from GTA's
//!   vehicles.meta files; written by the setup). The highlighted car is
//!   shown on a turntable with a camera on it: on the junkyard's display
//!   lot (`lot` lines, marked in game; the nearest junkyard within
//!   LOT_RANGE gets it), else in place of the player's own car, which is
//!   hidden meanwhile. The chosen car is put in the drive-thru.
//!
//! Each acts once per visit: drive out and back in to use it again.
//! Setting: `@shops`.

use crate::natives as n;
use crate::shv::{self, Vec3};
use std::rc::Rc;

const FILE: &str = "paradise_shops.txt";
const CARS_FILE: &str = "paradise_cars.txt";
/// How far above or below its box (m) a car still counts as inside.
const HEIGHT_SLACK: f32 = 1.5;
/// Thinner boxes (a mod's drive-through gate) are widened to this half
/// size (m) so a fast car can't skip them between frames.
const MIN_HALF: f32 = 1.5;
/// A car is previewed once it has been highlighted this long (ms), so
/// scrolling through the list doesn't spawn every car on the way.
const PREVIEW_DELAY: i32 = 200;
/// Turntable speed (degrees a second).
const SPIN: f32 = 25.0;
/// A display lot belongs to the nearest junkyard this close (m).
const LOT_RANGE: f32 = 150.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Junkyard,
    Repair,
    Gas,
    Paint,
}

impl Kind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "junkyard" => Some(Kind::Junkyard),
            "repair" => Some(Kind::Repair),
            "gas" => Some(Kind::Gas),
            "paint" => Some(Kind::Paint),
            _ => None,
        }
    }

    /// Map blip sprite: GTA's junkyard (radar_junkyard), wrench, jerry can
    /// and Los Santos Customs' spray can.
    fn sprite(self) -> i32 {
        match self {
            Kind::Junkyard => 527,
            Kind::Repair => 446,
            Kind::Gas => 361,
            Kind::Paint => 72,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Junkyard => "Junkyard",
            Kind::Repair => "Auto Repair",
            Kind::Gas => "Gas Station",
            Kind::Paint => "Paint Shop",
        }
    }
}

struct Shop {
    kind: Kind,
    centre: Vec3,
    /// The box's own axes (along, across, up; unit vectors) and its half
    /// size along each.
    axes: [Vec3; 3],
    half: [f32; 3],
    inside: bool,
    /// A junkyard's display lot: where and facing which way cars are shown.
    lot: Option<(Vec3, f32)>,
}

impl Shop {
    fn contains(&self, p: Vec3) -> bool {
        let d = Vec3::new(p.x - self.centre.x, p.y - self.centre.y, p.z - self.centre.z);
        let along = |a: Vec3| (d.x * a.x + d.y * a.y + d.z * a.z).abs();
        along(self.axes[0]) <= self.half[0].max(MIN_HALF) && along(self.axes[1]) <= self.half[1].max(MIN_HALF) && along(self.axes[2]) <= self.half[2] + HEIGHT_SLACK
    }
}

pub struct Shops {
    shops: Vec<Shop>,
    blips: Vec<i32>,
    message: Option<(String, i32)>,
    menu: Menu,
    /// The junkyard's cars, read the first time a menu opens.
    cars: Option<Rc<Vec<Car>>>,
    /// Paint shops' random colours (xorshift; seeded from the clock).
    rng: u32,
}

/// A vehicle the junkyard offers.
struct Car {
    model: String,
    hash: u32,
    class: i32,
    /// Its name in the game ("Truffade Adder").
    name: String,
    /// Stock top speed (m/s), acceleration, braking and traction, as the
    /// game rates them (`STATS`).
    perf: [f32; 4],
    /// The same as stat bars, 0..1 against the rest of the list.
    bars: [f32; 4],
}

const STATS: [&str; 4] = ["Top Speed", "Acceleration", "Braking", "Traction"];

impl Car {
    /// Its stats for the log: "speed 8/10 (44.2), ...".
    fn describe(&self) -> String {
        let names = ["speed", "acceleration", "braking", "traction"];
        let parts: Vec<String> = (0..4).map(|k| format!("{} {:.0}/10 ({:.2})", names[k], self.bars[k] * 10.0, self.perf[k])).collect();
        parts.join(", ")
    }
}

/// GTA's vehicle classes (GET_VEHICLE_CLASS), and the order the junkyard
/// lists them in. Cycles, boats, aircraft and trains aren't offered.
const CLASSES: [&str; 23] = [
    "Compacts", "Sedans", "SUVs", "Coupes", "Muscle", "Sports Classics", "Sports", "Super", "Motorcycles", "Off-road", "Industrial", "Utility", "Vans", "Cycles", "Boats", "Helicopters", "Planes", "Service", "Emergency", "Military", "Commercial", "Trains", "Open Wheel",
];
const CLASS_ORDER: [i32; 18] = [7, 6, 5, 4, 3, 1, 0, 2, 9, 8, 22, 12, 10, 11, 17, 18, 19, 20];

/// Every vehicle in paradise_cars.txt the game has, sorted by class order
/// and then name.
fn load_cars() -> Vec<Car> {
    let text = std::fs::read_to_string(CARS_FILE).unwrap_or_default();
    let mut missing = 0;
    let mut cars: Vec<Car> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|model| {
            let hash = crate::propshield::joaat(model);
            if !n::is_model_in_cdimage(hash) || !n::is_model_a_vehicle(hash) {
                missing += 1;
                return None;
            }
            let class = n::vehicle_class_from_name(hash);
            CLASS_ORDER.contains(&class).then(|| {
                let name = n::display_name(hash).unwrap_or_else(|| model.to_string());
                let name = match n::make_name(hash) {
                    Some(make) if !name.starts_with(&make) => format!("{make} {name}"),
                    _ => name,
                };
                Car { model: model.to_string(), hash, class, name, perf: n::model_performance(hash), bars: [0.0; 4] }
            })
        })
        .collect();
    let rank = |c: i32| CLASS_ORDER.iter().position(|&k| k == c).unwrap_or(99);
    cars.sort_by(|a, b| rank(a.class).cmp(&rank(b.class)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    crate::log(&format!("shops: junkyard car list: {} vehicles from {CARS_FILE} ({missing} not in this game)", cars.len()));
    // The bars run from the list's 2nd to its 98th percentile, so a few
    // freaks (rocket cars, tractors) don't squash everyone else together.
    let mut ranges = [(0.0f32, 1.0f32); 4];
    for (k, range) in ranges.iter_mut().enumerate() {
        let mut v: Vec<f32> = cars.iter().map(|c| c.perf[k]).filter(|&v| v > 0.0).collect();
        v.sort_by(f32::total_cmp);
        if v.len() >= 2 {
            let at = |p: f32| v[((v.len() - 1) as f32 * p) as usize];
            *range = (at(0.02), at(0.98));
        }
    }
    for c in &mut cars {
        for (k, &(lo, hi)) in ranges.iter().enumerate() {
            let v = c.perf[k];
            c.bars[k] = if v > 0.0 { 0.05 + 0.95 * ((v - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0) } else { 0.0 };
        }
    }
    let blank = cars.iter().filter(|c| c.perf.iter().all(|&v| v <= 0.0)).count();
    let shown: Vec<String> = ranges.iter().zip(STATS).map(|(&(lo, hi), name)| format!("{name} {lo:.2} to {hi:.2}")).collect();
    crate::log(&format!("shops: junkyard stat bars: {} (top speed in m/s); {blank} cars report no stats", shown.join(", ")));
    cars
}

impl Shops {
    pub fn load() -> Self {
        let text = std::fs::read_to_string(FILE).unwrap_or_default();
        let mut shops: Vec<Shop> = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let p: Vec<&str> = l.split_whitespace().collect();
                let f = |i: usize| p.get(i)?.parse::<f32>().ok();
                let v = |i: usize| Some(Vec3::new(f(i)?, f(i + 1)?, f(i + 2)?));
                Some(Shop { kind: Kind::parse(p.first()?)?, centre: v(1)?, axes: [v(4)?, v(7)?, v(10)?], half: [f(13)?, f(14)?, f(15)?], inside: false, lot: None })
            })
            .collect();
        for l in text.lines().filter(|l| l.starts_with("lot ")) {
            let v: Vec<f32> = l.split_whitespace().skip(1).filter_map(|f| f.parse().ok()).collect();
            let [x, y, z, heading] = v[..] else { continue };
            let d = |s: &Shop| (s.centre.x - x).hypot(s.centre.y - y);
            match shops.iter_mut().filter(|s| s.kind == Kind::Junkyard && d(s) < LOT_RANGE).min_by(|a, b| d(a).total_cmp(&d(b))) {
                Some(s) => s.lot = Some((Vec3::new(x, y, z), heading)),
                None => crate::log(&format!("shops: display lot ({x:.0}, {y:.0}) has no junkyard within {LOT_RANGE} m")),
            }
        }
        let count = |k: Kind| shops.iter().filter(|s| s.kind == k).count();
        let lots = shops.iter().filter(|s| s.lot.is_some()).count();
        crate::log(&format!("shops: {} junkyards ({lots} with a display lot), {} auto repairs, {} gas stations, {} paint shops from {FILE}", count(Kind::Junkyard), count(Kind::Repair), count(Kind::Gas), count(Kind::Paint)));
        Self { shops, blips: Vec::new(), message: None, menu: Menu::Closed, cars: None, rng: 0 }
    }

    /// The plugin's blips (the missions module leaves them alone).
    pub fn blips(&self) -> &[i32] {
        &self.blips
    }

    fn add_blips(&mut self) {
        for s in &self.shops {
            let b = n::add_blip_for_coord(s.centre);
            n::set_blip_sprite(b, s.kind.sprite());
            n::set_blip_as_short_range(b, true);
            n::set_blip_name(b, s.kind.name());
            self.blips.push(b);
        }
        crate::log(&format!("shops: {} map blips added", self.blips.len()));
    }

    pub fn remove_blips(&mut self) {
        for b in self.blips.drain(..) {
            n::remove_blip(b);
        }
    }

    /// A junkyard is stopping the car or its menu is open: the crash,
    /// boost and other driving systems should leave the car alone.
    pub fn busy(&self) -> bool {
        !matches!(self.menu, Menu::Closed)
    }

    pub fn update(&mut self, on: bool, ped: i32, driven: i32, boost: &mut crate::boost::Boost, now: i32) {
        if !on {
            self.close_menu();
            self.remove_blips();
            return;
        }
        if self.blips.is_empty() && !self.shops.is_empty() {
            self.add_blips();
        }
        if self.busy() {
            self.run_menu(ped, now);
        } else {
            let pos = if driven != 0 { Some(n::get_entity_coords(driven)) } else { None };
            for i in 0..self.shops.len() {
                let inside = pos.is_some_and(|p| self.shops[i].contains(p));
                let entered = inside && !self.shops[i].inside;
                self.shops[i].inside = inside;
                if entered {
                    self.enter(i, driven, boost, now);
                }
            }
        }
        if let Some((text, until)) = &self.message {
            if now < *until {
                n::draw_text_centred(text, 0.5, 0.78, 0.7, [255, 200, 40, 255]);
            } else {
                self.message = None;
            }
        }
    }

    /// A random number below `n`.
    fn random(&mut self, n: u32) -> u32 {
        if self.rng == 0 {
            self.rng = (n::get_game_timer() as u32) | 1;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x % n
    }

    fn say(&mut self, text: &str, now: i32) {
        self.message = Some((text.to_string(), now + 2500));
    }

    fn enter(&mut self, i: usize, car: i32, boost: &mut crate::boost::Boost, now: i32) {
        let s = &self.shops[i];
        let (kind, c, lot) = (s.kind, s.centre, s.lot);
        let at = format!("{} at ({:.0}, {:.0})", kind.name(), c.x, c.y);
        match kind {
            Kind::Repair => {
                let (engine, body) = (n::get_vehicle_engine_health(car), n::get_vehicle_body_health(car));
                n::repair_vehicle(car);
                n::play_sound_frontend("PURCHASE", "HUD_LIQUOR_STORE_SOUNDSET");
                self.say("REPAIRED", now);
                crate::log(&format!("shops: {at}: car repaired (engine was {engine:.0}, body {body:.0})"));
            }
            Kind::Gas => {
                let before = boost.gauge;
                boost.reward(1.0, "BOOST FILLED", now);
                n::play_sound_frontend("PURCHASE", "HUD_LIQUOR_STORE_SOUNDSET");
                crate::log(&format!("shops: {at}: boost filled (was {:.0}%)", before * 100.0));
            }
            Kind::Paint => {
                let (was, pearl_was) = (n::vehicle_colours(car), n::vehicle_extra_colours(car).0);
                let (a, b, pearl) = (self.random(PALETTE), self.random(PALETTE), self.random(PALETTE));
                n::set_custom_colour(car, false, None);
                n::set_custom_colour(car, true, None);
                n::set_vehicle_colours(car, a as i32, b as i32);
                n::set_vehicle_extra_colours(car, pearl as i32, n::vehicle_extra_colours(car).1);
                n::play_sound_frontend("PURCHASE", "HUD_LIQUOR_STORE_SOUNDSET");
                self.say("RESPRAYED", now);
                crate::log(&format!("shops: {at}: resprayed, colours {a}/{b} pearl {pearl} (were {}/{} pearl {pearl_was})", was.0, was.1));
            }
            Kind::Junkyard => {
                if n::get_player_wanted_level(n::player_id()) > 0 {
                    self.say("LOSE THE COPS FIRST", now);
                    crate::log(&format!("shops: {at}: skipped, the player is wanted"));
                } else {
                    let speed = n::get_entity_speed(car);
                    stop_dead(car);
                    self.menu = Menu::Stopping { car, lot };
                    crate::log(&format!("shops: {at}: stopped for the menu (came in at {speed:.1} m/s)"));
                }
            }
        }
    }

    fn close_menu(&mut self) {
        let car = match self.menu {
            Menu::Stopping { car, .. } => car,
            Menu::Open(ref mut m) => {
                m.end_preview();
                m.car
            }
            Menu::Closed => return,
        };
        if n::does_entity_exist(car) {
            n::freeze_entity_position(car, false);
        }
        self.menu = Menu::Closed;
    }

    fn run_menu(&mut self, ped: i32, now: i32) {
        let car = match self.menu {
            Menu::Stopping { car, .. } => car,
            Menu::Open(ref m) => m.car,
            Menu::Closed => return,
        };
        if !n::does_entity_exist(car) || n::is_entity_dead(ped) || !n::is_ped_in_any_vehicle(ped) || n::get_vehicle_ped_is_in(ped) != car {
            crate::log("shops: junkyard menu closed (left the car)");
            self.close_menu();
            return;
        }
        n::disable_all_controls();
        // Looking around the car is fine.
        for look in [1, 2, 0] {
            n::enable_control(look);
        }
        if let Menu::Stopping { car, lot } = self.menu {
            stop_dead(car);
            n::set_vehicle_mod_kit(car);
            n::play_sound_frontend("SELECT", "HUD_FRONTEND_DEFAULT_SOUNDSET");
            crate::log("shops: junkyard menu open");
            let cars = self.cars.get_or_insert_with(|| Rc::new(load_cars())).clone();
            self.menu = Menu::Open(MenuState::new(car, cars, lot));
            return;
        }
        let Menu::Open(m) = &mut self.menu else { return };
        if !m.frame(now) {
            crate::log(&format!("shops: junkyard menu closed ({} change(s))", m.changes));
            self.close_menu();
            self.say("ALL SET", now);
        }
    }
}

/// Stops a car dead where it is, however fast, so it stays in the
/// junkyard's drive-thru.
fn stop_dead(car: i32) {
    n::set_entity_velocity(car, 0.0, 0.0, 0.0);
    n::set_entity_angular_velocity(car, 0.0, 0.0, 0.0);
    n::freeze_entity_position(car, true);
}

enum Menu {
    Closed,
    /// Stopped in the junkyard (and its display lot); the menu opens next
    /// frame.
    Stopping { car: i32, lot: Option<(Vec3, f32)> },
    Open(MenuState),
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    /// Modify the car, or select a new one.
    Top,
    Modify,
    Respray,
    Wheels,
    Parts,
    Classes,
    /// The cars of one class.
    Cars(i32),
}

#[derive(Clone, Copy, PartialEq)]
enum Item {
    Open(Page, &'static str),
    Colour(bool),
    Finish,
    WheelType,
    Mod(i32),
    Toggle(i32, &'static str),
    Tint,
    Plate,
    Livery,
    Class(i32),
    /// Index in the car list.
    Car(usize),
    Done,
}

/// GTA's paint palette (colour indices 0..PALETTE) for paint shops.
const PALETTE: u32 = 160;

/// Paint for the respray (custom colours; the first is the car's own).
const PAINTS: [(&str, [i32; 3]); 20] = [
    ("Factory", [0, 0, 0]),
    ("White", [240, 240, 240]),
    ("Silver", [170, 175, 180]),
    ("Graphite", [60, 62, 66]),
    ("Black", [8, 8, 10]),
    ("Burnout Red", [200, 16, 22]),
    ("Crimson", [110, 8, 18]),
    ("Orange", [255, 110, 0]),
    ("Sunset Gold", [230, 165, 30]),
    ("Yellow", [250, 215, 0]),
    ("Lime", [140, 220, 20]),
    ("Racing Green", [10, 90, 40]),
    ("Teal", [0, 150, 140]),
    ("Sky Blue", [80, 170, 240]),
    ("Paradise Blue", [20, 80, 200]),
    ("Navy", [15, 25, 70]),
    ("Purple", [100, 30, 160]),
    ("Pink", [240, 80, 170]),
    ("Brown", [90, 55, 30]),
    ("Sand", [200, 175, 130]),
];
const FINISHES: [&str; 6] = ["Normal", "Metallic", "Pearlescent", "Matte", "Metal", "Chrome"];
const WHEEL_TYPES: [&str; 13] = ["Sport", "Muscle", "Lowrider", "SUV", "Offroad", "Tuner", "Bike", "High End", "Benny's Original", "Benny's Bespoke", "Open Wheel", "Street", "Track"];
const TINTS: [&str; 7] = ["None", "Pure Black", "Dark Smoke", "Light Smoke", "Stock", "Limo", "Green"];
const PLATES: [&str; 6] = ["Blue on White", "Yellow on Black", "Yellow on Blue", "Blue on White 2", "Blue on White 3", "Yankton"];
/// GTA's mod slots shown under Body & Performance (wheels, toggles and
/// liveries are elsewhere), with names for when the car has none.
const PARTS: [(i32, &str); 42] = [
    (0, "Spoiler"),
    (1, "Front Bumper"),
    (2, "Rear Bumper"),
    (3, "Skirts"),
    (4, "Exhaust"),
    (5, "Roll Cage"),
    (6, "Grille"),
    (7, "Hood"),
    (8, "Left Fender"),
    (9, "Right Fender"),
    (10, "Roof"),
    (11, "Engine"),
    (12, "Brakes"),
    (13, "Transmission"),
    (14, "Horn"),
    (15, "Suspension"),
    (16, "Armour"),
    (25, "Plate Holder"),
    (26, "Vanity Plate"),
    (27, "Trim"),
    (28, "Ornaments"),
    (29, "Dashboard"),
    (30, "Dials"),
    (31, "Door Speakers"),
    (32, "Seats"),
    (33, "Steering Wheel"),
    (34, "Shifter"),
    (35, "Plaques"),
    (36, "Speakers"),
    (37, "Trunk"),
    (38, "Hydraulics"),
    (39, "Engine Block"),
    (40, "Air Filter"),
    (41, "Struts"),
    (42, "Arch Covers"),
    (43, "Aerials"),
    (44, "Trim 2"),
    (45, "Tank"),
    (46, "Windows"),
    (47, "Doors"),
    (48, "Livery"),
    (24, "Rear Wheel"),
];
const RIMS: i32 = 23;
const CLASS_MOTORCYCLE: i32 = 8;
const CLASS_OPEN_WHEEL: i32 = 22;

// Frontend controls.
const UP: i32 = 188;
const DOWN: i32 = 187;
const LEFT: i32 = 189;
const RIGHT: i32 = 190;
const ACCEPT: i32 = 201;
const CANCEL: i32 = 202;
const ROWS: usize = 12;

struct MenuState {
    car: i32,
    cars: Rc<Vec<Car>>,
    page: Page,
    items: Vec<Item>,
    cursor: usize,
    /// The pages (and cursors) Back returns to.
    back: Vec<(Page, usize)>,
    /// Where the car stands and faces in the junkyard.
    spot: Vec3,
    heading: f32,
    /// The junkyard's display lot, if it has one marked.
    lot: Option<(Vec3, f32)>,
    /// The previewed car (an index in `cars`) and its vehicle.
    preview: Option<(usize, i32)>,
    /// The car waiting to be previewed, and since when.
    want: Option<(usize, i32)>,
    /// The turntable's angle (degrees).
    spin: f32,
    cam: i32,
    /// The player and their car are hidden behind the preview.
    hidden: bool,
    paint: [usize; 2],
    /// A held direction: (control, when it repeats next).
    held: Option<(i32, i32)>,
    changes: u32,
}

impl MenuState {
    fn new(car: i32, cars: Rc<Vec<Car>>, lot: Option<(Vec3, f32)>) -> Self {
        let (spot, heading) = (n::get_entity_coords(car), n::get_entity_heading(car));
        let spin = lot.map_or(heading, |(_, h)| h);
        let mut m = MenuState { car, cars, page: Page::Top, items: Vec::new(), cursor: 0, back: Vec::new(), spot, heading, lot, preview: None, want: None, spin, cam: 0, hidden: false, paint: [0, 0], held: None, changes: 0 };
        m.read_paint();
        m.items = m.page_items();
        m
    }

    /// Which of the respray's paints the car has.
    fn read_paint(&mut self) {
        let paint_of = |secondary| n::custom_colour(self.car, secondary).and_then(|c| PAINTS.iter().position(|p| p.1 == c)).unwrap_or(0);
        self.paint = [paint_of(false), paint_of(true)];
    }

    /// Where cars are shown: the display lot, else the drive-thru.
    fn stage(&self) -> (Vec3, f32) {
        self.lot.unwrap_or((self.spot, self.heading))
    }

    /// Hides (or shows again) the player and their car behind the preview.
    fn hide_own(&mut self, hide: bool) {
        if self.hidden != hide {
            self.hidden = hide;
            n::set_entity_visible(self.car, !hide);
            n::set_entity_collision(self.car, !hide);
            n::set_entity_visible(n::player_ped_id(), !hide);
        }
    }

    fn drop_preview(&mut self) {
        if let Some((_, v)) = self.preview.take() {
            n::delete_vehicle(v);
        }
    }

    /// The camera on the stage, far enough back for a car of `model`'s
    /// size: from the side with the most room (the lots' marked headings
    /// aren't a guide to that), else kept in front of what's in the way.
    fn place_cam(&mut self, model: u32) {
        if self.cam == 0 {
            self.cam = n::create_cam();
            n::set_cam_fov(self.cam, 45.0);
            n::set_cam_active(self.cam, true);
            n::render_script_cams(true, 400);
        }
        let (lo, hi) = n::model_dimensions(model);
        let dist = ((hi.y - lo.y) * 0.9 + 2.5).clamp(5.0, 16.0);
        let (spot, heading) = self.stage();
        let target = Vec3::new(spot.x, spot.y, spot.z + 0.2);
        let ignore = self.preview.map_or(self.car, |(_, v)| v);
        let lift = 0.8 + (hi.z - lo.z) * 0.35;
        // (camera spot, clear distance), trying every 30 degrees round from
        // front-left of the stage's heading; the first fully clear one wins.
        let mut best: Option<(Vec3, f32)> = None;
        for k in 0..12 {
            let a = (heading + 35.0 + 30.0 * k as f32).to_radians();
            let at = Vec3::new(target.x - a.sin() * dist, target.y + a.cos() * dist, target.z + lift);
            let (dx, dy, dz) = (at.x - target.x, at.y - target.y, at.z - target.z);
            let len = (dx * dx + dy * dy + dz * dz).sqrt().max(0.01);
            let (spot, clear) = match n::raycast(target, at, 1, ignore) {
                Some(h) => {
                    let d = ((h.pos.x - target.x).powi(2) + (h.pos.y - target.y).powi(2) + (h.pos.z - target.z).powi(2)).sqrt();
                    (Vec3::new(h.pos.x - dx / len * 0.4, h.pos.y - dy / len * 0.4, h.pos.z - dz / len * 0.4), d)
                }
                None => (at, len),
            };
            if best.is_none_or(|(_, c)| clear > c + 0.1) {
                best = Some((spot, clear));
            }
            if clear >= len - 0.01 {
                break;
            }
        }
        let at = best.map_or(target, |(p, _)| p);
        n::set_cam_coord(self.cam, at);
        n::point_cam_at_coord(self.cam, target);
    }

    /// Ends the preview: the player's own car back, the normal camera.
    fn end_preview(&mut self) {
        self.drop_preview();
        self.hide_own(false);
        self.want = None;
        if self.cam != 0 {
            n::render_script_cams(false, 400);
            n::destroy_cam(self.cam);
            self.cam = 0;
        }
    }

    /// On a car list, shows the highlighted car on the turntable (without a
    /// display lot, the player's own car when it's that one); anywhere
    /// else, ends the preview.
    fn update_preview(&mut self, now: i32) {
        let Some(&Item::Car(i)) = self.items.get(self.cursor).filter(|_| matches!(self.page, Page::Cars(_))) else {
            self.end_preview();
            return;
        };
        let hash = self.cars[i].hash;
        if self.lot.is_none() && hash == n::get_entity_model(self.car) {
            self.drop_preview();
            self.hide_own(false);
            self.want = None;
            if self.cam == 0 {
                self.place_cam(hash);
            }
        } else if self.preview.map(|(c, _)| c) != Some(i) {
            if self.want.map(|(c, _)| c) != Some(i) {
                self.want = Some((i, now));
            }
            n::request_model(hash);
            let since = self.want.map_or(now, |(_, t)| t);
            if now - since >= PREVIEW_DELAY && n::has_model_loaded(hash) {
                self.drop_preview();
                self.hide_own(self.lot.is_none());
                let v = n::create_vehicle(hash, self.stage().0, self.spin);
                n::set_model_as_no_longer_needed(hash);
                if v != 0 {
                    n::set_vehicle_on_ground_properly(v);
                    n::freeze_entity_position(v, true);
                    self.preview = Some((i, v));
                    self.place_cam(hash);
                }
            }
        }
        self.spin = (self.spin + SPIN * n::get_frame_time()) % 360.0;
        if let Some((_, v)) = self.preview {
            n::set_entity_heading(v, self.spin);
        }
    }

    /// Swaps the player's car for a new `car` in the same spot (the
    /// previewed one, when it's on show).
    fn swap(&mut self, car: usize) {
        let (model, hash, name) = (self.cars[car].model.clone(), self.cars[car].hash, self.cars[car].name.clone());
        let stats = self.cars[car].describe();
        let ped = n::player_ped_id();
        if let Some((c, new)) = self.preview.take() {
            if c == car {
                let old_model = n::get_entity_model(self.car);
                n::set_entity_visible(ped, true);
                n::set_ped_into_vehicle(ped, new);
                n::delete_vehicle(self.car);
                self.hidden = false;
                n::set_entity_coords(new, self.spot);
                n::set_entity_heading(new, self.heading);
                n::set_vehicle_on_ground_properly(new);
                n::freeze_entity_position(new, true);
                n::set_vehicle_engine_on(new);
                n::set_vehicle_mod_kit(new);
                n::set_vehicle_as_no_longer_needed(new);
                self.car = new;
                self.read_paint();
                self.changes += 1;
                crate::log(&format!("shops: junkyard: swapped model 0x{old_model:08X} for {model} ({name}), the previewed car; {stats}"));
                return;
            }
            n::delete_vehicle(new);
        }
        self.hide_own(false);
        let t0 = n::get_game_timer();
        n::request_model(hash);
        while !n::has_model_loaded(hash) && n::get_game_timer() - t0 < 5000 {
            n::request_model(hash);
            n::disable_all_controls();
            self.draw();
            n::draw_text_centred(&format!("Loading the {name}..."), 0.5, 0.78, 0.6, [255, 200, 40, 255]);
            shv::wait(0);
        }
        if !n::has_model_loaded(hash) {
            crate::log(&format!("shops: junkyard: '{model}' didn't load"));
            return;
        }
        let (pos, heading) = (n::get_entity_coords(self.car), n::get_entity_heading(self.car));
        let new = n::create_vehicle(hash, Vec3::new(pos.x, pos.y, pos.z + 50.0), heading);
        n::set_model_as_no_longer_needed(hash);
        if new == 0 {
            crate::log(&format!("shops: junkyard: couldn't create '{model}'"));
            return;
        }
        // Into the new car (waiting above) first, then the old one goes and
        // the new one takes its place.
        n::freeze_entity_position(new, true);
        n::set_ped_into_vehicle(n::player_ped_id(), new);
        let old_model = n::get_entity_model(self.car);
        n::delete_vehicle(self.car);
        n::set_entity_coords(new, pos);
        n::set_entity_heading(new, heading);
        n::set_vehicle_on_ground_properly(new);
        n::set_vehicle_engine_on(new);
        n::set_vehicle_mod_kit(new);
        n::set_vehicle_as_no_longer_needed(new);
        self.car = new;
        self.read_paint();
        self.changes += 1;
        crate::log(&format!("shops: junkyard: swapped model 0x{old_model:08X} for {model} ({name}); {stats}"));
    }

    fn page_items(&self) -> Vec<Item> {
        let v = self.car;
        match self.page {
            Page::Top => vec![Item::Open(Page::Modify, "Modify Vehicle"), Item::Open(Page::Classes, "Select a New Car"), Item::Done],
            Page::Classes => CLASS_ORDER.iter().filter(|&&c| self.cars.iter().any(|car| car.class == c)).map(|&c| Item::Class(c)).collect(),
            Page::Cars(class) => (0..self.cars.len()).filter(|&i| self.cars[i].class == class).map(Item::Car).collect(),
            Page::Modify => {
                let mut items = vec![Item::Open(Page::Respray, "Respray"), Item::Open(Page::Wheels, "Wheels"), Item::Open(Page::Parts, "Body & Performance"), Item::Tint, Item::Plate];
                if n::livery_count(v) > 1 {
                    items.push(Item::Livery);
                }
                items.push(Item::Toggle(18, "Turbo"));
                items.push(Item::Toggle(22, "Xenon Headlights"));
                items.push(Item::Done);
                items
            }
            Page::Respray => vec![Item::Colour(false), Item::Colour(true), Item::Finish],
            Page::Wheels => vec![Item::WheelType, Item::Mod(RIMS)],
            Page::Parts => PARTS.iter().filter(|(k, _)| n::num_vehicle_mods(v, *k) > 0).map(|&(k, _)| Item::Mod(k)).collect(),
        }
    }

    fn label(&self, item: Item) -> String {
        match item {
            Item::Open(_, name) | Item::Toggle(_, name) => name.to_string(),
            Item::Colour(false) => "Primary Colour".into(),
            Item::Colour(true) => "Secondary Colour".into(),
            Item::Finish => "Finish".into(),
            Item::WheelType => "Rim Type".into(),
            Item::Mod(RIMS) => "Rims".into(),
            Item::Mod(k) => n::mod_slot_name(self.car, k).unwrap_or_else(|| PARTS.iter().find(|p| p.0 == k).map_or(format!("Part {k}"), |p| p.1.to_string())),
            Item::Tint => "Window Tint".into(),
            Item::Plate => "Plate".into(),
            Item::Livery => "Livery".into(),
            Item::Class(c) => CLASSES[c as usize].into(),
            Item::Car(i) => self.cars[i].name.clone(),
            Item::Done => "Done".into(),
        }
    }

    fn value(&self, item: Item) -> Option<String> {
        let v = self.car;
        Some(match item {
            Item::Open(..) | Item::Done => return None,
            Item::Class(c) => format!("{} >", self.cars.iter().filter(|car| car.class == c).count()),
            Item::Car(i) if self.cars[i].hash == n::get_entity_model(v) => "Driving".into(),
            Item::Car(_) => return None,
            Item::Colour(secondary) => PAINTS[self.paint[secondary as usize]].0.to_string(),
            Item::Finish => FINISHES.get(n::paint_finish(v) as usize).unwrap_or(&"Other").to_string(),
            Item::WheelType => WHEEL_TYPES.get(n::vehicle_wheel_type(v) as usize).unwrap_or(&"Other").to_string(),
            Item::Mod(k) => {
                let i = n::vehicle_mod(v, k);
                if i < 0 {
                    "Stock".into()
                } else {
                    n::mod_name(v, k, i).unwrap_or_else(|| match k {
                        11 | 12 | 13 | 15 => format!("Level {}", i + 1),
                        16 => format!("{}%", (i + 1) * 20),
                        _ => format!("Option {}", i + 1),
                    })
                }
            }
            Item::Toggle(k, _) => if n::is_toggle_mod_on(v, k) { "On" } else { "Off" }.into(),
            Item::Tint => TINTS.get(n::vehicle_window_tint(v) as usize).unwrap_or(&"Stock").to_string(),
            Item::Plate => PLATES.get(n::plate_style(v) as usize).unwrap_or(&"Other").to_string(),
            Item::Livery => format!("{} of {}", n::livery(v) + 1, n::livery_count(v)),
        })
    }

    /// Steps an item's value by `d` (+1 / -1), wrapping around.
    fn change(&mut self, item: Item, d: i32) -> bool {
        let v = self.car;
        let step = |cur: i32, lo: i32, count: i32| lo + (cur - lo + d).rem_euclid(count.max(1));
        match item {
            Item::Colour(secondary) => {
                let i = &mut self.paint[secondary as usize];
                *i = step(*i as i32, 0, PAINTS.len() as i32) as usize;
                n::set_custom_colour(v, secondary, (*i > 0).then_some(PAINTS[*i].1));
            }
            Item::Finish => {
                n::set_paint_finish(v, step(n::paint_finish(v), 0, FINISHES.len() as i32));
                // Put the custom colour back over it, in case the finish
                // change cleared it.
                let i = self.paint[0];
                if i > 0 {
                    n::set_custom_colour(v, false, Some(PAINTS[i].1));
                }
            }
            Item::WheelType => {
                let class = n::get_vehicle_class(v);
                let allowed: Vec<i32> = (0..WHEEL_TYPES.len() as i32)
                    .filter(|&t| if class == CLASS_MOTORCYCLE { t == 6 } else { t != 6 && (t != 10 || class == CLASS_OPEN_WHEEL) })
                    .collect();
                let at = allowed.iter().position(|&t| t == n::vehicle_wheel_type(v)).unwrap_or(0) as i32;
                n::set_vehicle_wheel_type(v, allowed[step(at, 0, allowed.len() as i32) as usize]);
                n::set_vehicle_mod(v, RIMS, 0);
            }
            Item::Mod(k) => n::set_vehicle_mod(v, k, step(n::vehicle_mod(v, k), -1, n::num_vehicle_mods(v, k) + 1)),
            Item::Toggle(k, _) => n::toggle_vehicle_mod(v, k, !n::is_toggle_mod_on(v, k)),
            Item::Tint => n::set_vehicle_window_tint(v, step(n::vehicle_window_tint(v).max(0), 0, TINTS.len() as i32)),
            Item::Plate => n::set_plate_style(v, step(n::plate_style(v), 0, PLATES.len() as i32)),
            Item::Livery => n::set_livery(v, step(n::livery(v), 0, n::livery_count(v))),
            Item::Open(..) | Item::Class(_) | Item::Car(_) | Item::Done => return false,
        }
        self.changes += 1;
        true
    }

    fn pressed(&mut self, control: i32, now: i32) -> bool {
        if n::is_disabled_control_just_pressed(control) {
            self.held = Some((control, now + 400));
            return true;
        }
        match self.held {
            Some((c, at)) if c == control && n::is_disabled_control_pressed(control) && now >= at => {
                self.held = Some((c, now + 90));
                true
            }
            Some((c, _)) if c == control && !n::is_disabled_control_pressed(control) => {
                self.held = None;
                false
            }
            _ => false,
        }
    }

    fn go(&mut self, page: Page) {
        self.back.push((self.page, self.cursor));
        self.page = page;
        self.items = self.page_items();
        self.cursor = 0;
    }

    /// Back a page; false on the first page.
    fn go_back(&mut self) -> bool {
        let Some((page, cursor)) = self.back.pop() else { return false };
        self.page = page;
        self.items = self.page_items();
        self.cursor = cursor.min(self.items.len().saturating_sub(1));
        true
    }

    /// One frame of the open menu; false once it's closed.
    fn frame(&mut self, now: i32) -> bool {
        let sound = |name: &str| n::play_sound_frontend(name, "HUD_FRONTEND_DEFAULT_SOUNDSET");
        if self.items.is_empty() {
            self.items.push(Item::Done);
        }
        let len = self.items.len();
        if self.pressed(UP, now) {
            self.cursor = (self.cursor + len - 1) % len;
            sound("NAV_UP_DOWN");
        }
        if self.pressed(DOWN, now) {
            self.cursor = (self.cursor + 1) % len;
            sound("NAV_UP_DOWN");
        }
        let item = self.items[self.cursor];
        for (control, d) in [(LEFT, -1), (RIGHT, 1)] {
            if self.pressed(control, now) && self.change(item, d) {
                sound("NAV_LEFT_RIGHT");
            }
        }
        if n::is_disabled_control_just_pressed(ACCEPT) {
            match item {
                Item::Open(page, _) => {
                    self.go(page);
                    sound("SELECT");
                }
                Item::Class(c) => {
                    self.go(Page::Cars(c));
                    sound("SELECT");
                }
                Item::Car(i) => {
                    sound("SELECT");
                    self.swap(i);
                }
                Item::Done => {
                    sound("SELECT");
                    return false;
                }
                _ => {
                    if self.change(item, 1) {
                        sound("NAV_LEFT_RIGHT");
                    }
                }
            }
        }
        if n::is_disabled_control_just_pressed(CANCEL) {
            sound("BACK");
            if !self.go_back() {
                return false;
            }
        }
        self.update_preview(now);
        self.draw();
        true
    }

    fn draw(&self) {
        let (x, w, row) = (0.04, 0.30, 0.034);
        let title = match self.page {
            Page::Top => "JUNKYARD".to_string(),
            Page::Modify => "MODIFY VEHICLE".into(),
            Page::Respray => "RESPRAY".into(),
            Page::Wheels => "WHEELS".into(),
            Page::Parts => "BODY & PERFORMANCE".into(),
            Page::Classes => "SELECT A NEW CAR".into(),
            Page::Cars(c) => CLASSES[c as usize].to_uppercase(),
        };
        let top = 0.12;
        n::draw_rect(x + w / 2.0, top, w, 0.06, [215, 90, 10, 235]);
        n::draw_text(&title, x + 0.01, top - 0.022, 0.6, [255, 255, 255, 255]);
        let first = self.cursor.saturating_sub(ROWS - 1);
        let shown = &self.items[first..self.items.len().min(first + ROWS)];
        for (i, &item) in shown.iter().enumerate() {
            let y = top + 0.03 + row * (i as f32 + 0.5);
            let selected = first + i == self.cursor;
            n::draw_rect(x + w / 2.0, y, w, row, if selected { [240, 240, 240, 230] } else { [0, 0, 0, 170] });
            let colour = if selected { [20, 20, 20, 255] } else { [255, 255, 255, 255] };
            n::draw_text_outlined(&self.label(item), x + 0.008, y - 0.013, 0.35, colour, !selected);
            let value = match item {
                Item::Open(..) => Some(">".to_string()),
                _ => self.value(item).map(|v| if selected { format!("< {v} >") } else { v }),
            };
            if let Some(v) = value {
                n::draw_text_outlined(&v, x + 0.165, y - 0.013, 0.35, colour, !selected);
            }
        }
        let bottom = top + 0.03 + row * shown.len() as f32;
        n::draw_rect(x + w / 2.0, bottom + 0.016, w, 0.032, [0, 0, 0, 200]);
        let more = if self.items.len() > ROWS { format!("  ({}/{})", self.cursor + 1, self.items.len()) } else { String::new() };
        let help = if matches!(self.page, Page::Cars(_)) { "Up/Down choose   Enter drive it   Back" } else { "Up/Down choose   Left/Right change   Enter open   Back" };
        n::draw_text(&format!("{help}{more}"), x + 0.008, bottom + 0.004, 0.3, [220, 220, 220, 255]);
        if let (Page::Cars(_), Some(&Item::Car(i))) = (self.page, self.items.get(self.cursor)) {
            self.draw_stats(i, x, w, bottom + 0.04);
        }
    }

    /// The highlighted car's stat bars, under the list, with white marks
    /// where the car being driven comes.
    fn draw_stats(&self, i: usize, x: f32, w: f32, top: f32) {
        const SEGMENTS: usize = 10;
        let (row, bar_x, bar_w) = (0.03, x + 0.11, 0.17);
        let (seg, inner) = (bar_w / SEGMENTS as f32, bar_w / SEGMENTS as f32 - 0.002);
        let driving = n::get_entity_model(self.car);
        let mine = self.cars.iter().find(|c| c.hash == driving && c.hash != self.cars[i].hash);
        let height = row * STATS.len() as f32 + 0.012 + if mine.is_some() { 0.026 } else { 0.0 };
        n::draw_rect(x + w / 2.0, top + height / 2.0, w, height, [0, 0, 0, 170]);
        for (k, name) in STATS.iter().enumerate() {
            let y = top + 0.006 + row * (k as f32 + 0.5);
            n::draw_text(name, x + 0.008, y - 0.012, 0.32, [255, 255, 255, 255]);
            let fill = self.cars[i].bars[k] * SEGMENTS as f32;
            for s in 0..SEGMENTS {
                let left = bar_x + seg * s as f32;
                n::draw_rect(left + inner / 2.0, y, inner, 0.012, [70, 70, 70, 220]);
                let f = (fill - s as f32).clamp(0.0, 1.0);
                if f > 0.0 {
                    n::draw_rect(left + inner * f / 2.0, y, inner * f, 0.012, [235, 120, 20, 255]);
                }
            }
            if let Some(c) = mine {
                n::draw_rect(bar_x + bar_w * c.bars[k], y, 0.002, 0.02, [255, 255, 255, 255]);
            }
        }
        if mine.is_some() {
            n::draw_text("White marks: the car you're driving", x + 0.008, top + 0.006 + row * STATS.len() as f32, 0.28, [200, 200, 200, 255]);
        }
    }
}
