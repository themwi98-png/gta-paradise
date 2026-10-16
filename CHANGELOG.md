# GTA Paradise v0.2.1

A patch: flying through the windscreen in wrecks, a better brake-to-drift,
stronger brakes, and 1,439 more water fixes.

---

## Wrecks

- **Fly through the windscreen.** GTA's own windscreen ejection is back, but
  only in a wreck: in a big head-on crash you can be thrown out of the car,
  and the slow-motion camera follows both the car and you as you fly apart.
  You're unhurt and respawn in the new car as usual. Normal crashes and prop
  hits at speed never throw you out. `@wreck_eject` (on; off = stay in).

## Driving

- **Brake to drift only when turning.** Tapping the brake starts a drift only
  if you're steering (or already swinging round a corner) when you press it.
  Braking going straight or mostly straight is always just the brake.
  `@brake_drift_steer` (0.3): how far you must be steering, 0 to 1 (0 = the
  old always-on behaviour).
- **Stronger brakes:** 2.5× GTA's (was 1.75×, `@car_brakes` *(setup)*). Police
  and traffic use the same car data, so they brake just as hard.
- **Start in a Drift Tampa.** When Paradise City loads you're put in a fresh
  Drift Tampa on the road by the start point. `@start_car` (drifttampa): any
  GTA vehicle model name, or `none` to start on foot. Wrecks respawn you in a
  copy of whatever you're driving.

## Police

- **Pursuing police no longer get double engine power** (`@police_power`
  2 → 1). They keep their extra top speed (`@police_top` 100).

## Map

- **1,439 more flooded water squares removed** (2,095 in all).
- **Sidewalks fixed in 35 places:** 27 wrong sidewalk areas removed and 8
  new ones added, so pedestrians walk where they should.
- **More traffic fixes:** more lanes removed or redrawn, roads with faster
  traffic, and 11 more parked cars.

## Map editor (viewer)

- **Parked vehicles of any kind.** Set a parked spot to any GTA vehicle model
  (planes, helicopters, boats, ATVs...) with **Set vehicle**, or type a model
  before **Add parked car**. Shown orange.
- **Copy and paste parked vehicles** (Ctrl+C / Ctrl+V or the buttons): each
  paste lands 3 m to the side of the last, so repeated pastes make a row.
- **Parked cars are easier to click:** anywhere on their outline box counts.
- **Lane speeds show GTA's speed band** (slow under 40 km/h, normal 40–65,
  fast 65–97, fastest above). GTA traffic only drives differently when a
  lane changes band.

## Upgrading from v0.2.0

Run the new setup over your install. Settings you changed keep your values;
`@police_power` and `@car_brakes` move to the new defaults if you never
changed them, and the new settings are added.

---

# GTA Paradise v0.2.0

Major improvements to driving: drifting has been implemented in a way much more akin to Burnout, airborne vehicles now stay stable, fall faster, and barrel rolls are implemented like Burnout. Traffic now thins out in the countryside, off-road is as
fast as the road, and 270 collision fixes.

Every new setting lives in `paradise.ini` next to `GTA5.exe`. Most apply live
while you play; the ones marked *(setup)* are built into the car data, so run
the setup again after changing them.

---

## Crashes and wrecks

- **The Burnout rule: if it changes how the car drives, you're wrecked.**
  Smashed-up looks don't matter, but anything preventing the wheels from spinning now wrecks the car
  and you get a fresh one.
  - `@wreck_engine` (100): engine health below this wrecks the car (1000 = new;
    0 = only a dead engine).
  - `@wreck_wheel` (0 = off): a wheel knocked out of line (bent axle, wheel
    pushed in or torn off) can wreck the car too. Off by default; set it to how
    many cm out of line counts (8 is a good start) to try it.
  - `@strong_axles` (on): your axles don't bend from everyday knocks; only a
    crash that crushes the body in around a wheel moves it.
  - `@strong_tyres` (on): your tyres can't burst.
- **Crash damage reworked.** Dents are deeper on every car and battered cars keep driving until something that matters breaks.
- **Crash camera on every big hit.** Any impact of at least 72 km/h of sudden
  speed loss (`@crash_cam_speed` 20 m/s) shows the slow-motion crash camera
  from the moment of impact for `@crash_cam_hold` (1.5 s). If it wasn't a
  wreck, you get **DRIVEAWAY**. The pre-crash slow motion still only starts for
  wrecks that are certain.
- **No "wasted" on top of a wreck.** Drivers can no longer die in a crash or
  be thrown out of the car.
- **The clock stays put.** Getting wasted or busted no longer jumps the time of
  day forward by hours.

## Driving

- **Retuned cars.**
  - Brakes are much stronger: 1.75× GTA's (was 1.25×), and now a setting,
    `@car_brakes` *(setup)*.
  - Less grip, so cars slide more readily: cornering and sliding grip is 1.2×
    GTA's (was 1.4×), set with `@car_grip` *(setup)*.
  - Engine power is 1.35× GTA's (was 1.5×). Top speed is still 1.35×.
  - Engine power and top speed are now live settings: `@car_power` (1.35) and
    `@car_top_speed` (1.35), both times GTA's. Boost power follows
    `@car_power`.
  - Downforce at speed is a bit lighter (`@downforce` 1 → 0.75).
- **Drift assist.** Holds a slide and softens the catch. 
  - `@drift_assist` (1.1): how strongly it helps (0 = off, 1 = strong, up to 3).
  - `@drift_angle` (45): the largest drift angle, in degrees, it lets a slide
    reach.
- **Brake to drift, like Burnout.** Tap the normal brake while moving to start
  a drift: for the first moment it works as the handbrake, then as the normal
  brake if you keep holding it.
  - Already holding the handbrake? The brake stays a plain brake.
  - Pressing the handbrake during the drift switches to the normal brake
    instead.
  - `@brake_drift` (on), `@brake_drift_time` (0.5 s), `@brake_drift_speed`
    (2.2 m/s, about 8 km/h: the slowest speed it works from).
- **Off-road is as fast as the road.** Sand, dirt, gravel, grass, mud, rock and
  every other ground type no longer slow the car or cap its top speed. Grip
  still varies with the surface.

## Boost

- **Boost lasts more than twice as long:** a full gauge is 15 s (was 7 s,
  `@boost_seconds`).
- **Much higher boosted top speed:** up to 135 m/s, about 300 mph (was 95 m/s,
  `@boost_max_top`).

## Jumps

- **Steady flight.** In the air your car no longer pitches or spins about: it
  keeps its nose the way it's flying (following the arc only partway, so
  landings stay flat) and its wheels down. Small hops over bumps are left
  alone. `@air_control` (0.7; 0 = off, 1 = held steady).
- **Barrel rolls.** Leave a split ramp rolling, in either direction, and the
  car turns exactly once in the air and lands on its wheels, Burnout style.
  Straight jumps don't count. `@barrel_roll` (on); `@barrel_roll_trigger` (90)
  sets how hard a ramp must roll the car (higher = fewer rolls).
- **Heavier jumps.** Cars in the air now fall at 3× gravity (was 2×,
  `@air_gravity`).

## Props

- **Going through props doesn't slow car.** Lamp posts, benches, signs, bins
  and the rest still get knocked over and go flying but won't slow you down. 

## Traffic and police

- **Traffic thins out in the countryside.** Traffic levels now follow
  Burnout's own data for every lane: busy downtown, quiet country roads.
  Before, every road had the same amount.
- **Overall traffic setting.** `@traffic` (1): the amount of traffic, from 0
  (empty roads) to 3. It changes how much traffic there is, not where.

## Map

- **270 inbounds collision fixes** 

- **Falling through the map** now puts you back near where it happened (the
  last place you had ground under you) instead of at the spawn point.

## Map editor (viewer)

- **Traffic levels.** In the traffic panel:
  - **Colour by traffic** shows how busy every road is (grey none, blue quiet …
    red busy).
  - **Select area** selects every lane in a box (two clicks on the ground).
  - **Set traffic** changes the level of the selected lanes (Burnout's scale,
    0–9). Drawn lanes start at 5.

## Settings file

- `paradise.ini` is reorganised into sections, and every setting now shows its
  default value.
- If you delete `paradise.ini`, the setup puts a fresh one back.
- `paradise.log` now says what caused each wreck (speeds, what was hit...),
  handy when a wreck feels wrong.

---

## Upgrading from v0.1.0

Run the new setup over your existing install. Your `paradise.ini` comes along:
settings you changed keep your values, settings you never touched move to the
new defaults, and new settings are added. To start over with every default,
delete `paradise.ini` and run the setup again.
