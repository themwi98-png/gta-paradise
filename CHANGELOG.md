# GTA Paradise v0.3.0

Takedowns, police that fight back, Paradise City on the radar and pause map
with GPS routes, Burnout's drive-thrus (junkyards with a car menu, repairs,
gas and paint shops), working traffic lights, grinding, and the distant
mountains and low-detail city.

Every new setting lives in `paradise.ini` next to `GTA5.exe`. Most apply live
while you play; the ones marked *(setup)* are built into the car data, so run
the setup again after changing them.

---

## Takedowns

- **Ram police cars into a crash to take them down.** A car you hit stays
  vulnerable for a moment (`@takedown_window` 2 s): if it crashes, rolls or
  spins out in that time, it's your takedown, with Burnout's banner (SLAM,
  SHUNT, T-BONE, VERTICAL, ROADBLOCK, DOUBLE / TRIPLE TAKEDOWN), a sound and
  half a boost gauge (`@takedown_boost`).
- **Hard hits take them down at once** (`@takedown_smash` 20 m/s closing
  speed), parked roadblock cars included. Taken-down cars are thrown clear
  of your path (`@takedown_launch`) and blow up (`@takedown_explode`).
- **Any vehicle can be taken down**, not only police (`@takedown_any`).
- **Police can take you down too:** a wreck right after a police car hit you
  is TAKEN DOWN and costs a quarter of the gauge (`@taken_down_boost`).
- **Five police takedowns in one chase and you escape:** YOU ESCAPED!, wanted
  level gone (`@chase_takedowns`).
- **Your car stays straight while ramming:** it can't be spun, and keeps
  pointing and moving the way it was going; steering still turns it, and
  the ram's slowdown stays (`@takedown_straight`, `@takedown_keep`).
- Ramming from behind or alongside never wrecks you (head-ons still can),
  and you're never thrown through the windscreen near police.

## Police

- **They come for you:** police cars near you go for a PIT from behind or a
  slam from alongside, and oncoming ones steer into you for a head-on
  (`@police_ram` 12 m/s, `@police_ram_oncoming`, `@police_rammers` 4).
- **They keep up:** no more spinning out at speed (`@police_grip`), more pull
  when they speed up (`@police_accel`) and 3× brakes (`@police_brakes`).
  Flipped or stuck police cars are put back on the nearest road facing you.
- **Harder to lose:** while a police car is within 400 m it knows where you
  are, seen or not (`@police_sight`).
- **They only break when you break them:** police cars can't be damaged
  until your car hits them (`@police_tough`).
- The police car that took you down is left in place by the wreck clear-up.

## Around Paradise City

- **Burnout's drive-thrus, with map blips:**
  - **Auto Repair:** fixes your car.
  - **Gas Station:** fills the boost gauge.
  - **Paint Shop:** resprays the car in random colours, as in Burnout.
  - **Junkyard:** stops you and opens a menu. **Modify Vehicle**: respray
    (19 colours plus the factory paint, and finishes), wheels and rims,
    every body and performance part the car has, window tint, plate,
    livery, turbo and xenons. **Select a New Car**: any of GTA's road
    vehicles by class (776 in the current Legacy version), shown on a
    turntable in the junkyard's display lot with bars for top speed,
    acceleration, braking and traction; Enter and you drive it away. Not
    while wanted.
  - `@shops` (on).
- **GTA's missions are gone:** story missions, side activities, random
  events and their map blips are stopped and hidden, since their places
  were in Los Santos (`@gta_missions` off).

## Radar, pause map and GPS

- **The radar and pause map show Paradise City:** roads, buildings,
  railways, lakes and the coast, built from Burnout's map. The coast is
  hand-traced, including Silver Lake and Big Surf Island.
- **GPS routes over Paradise roads:** set a waypoint and the route is drawn
  on the radar, rerouting when you leave it.

## Driving

- **Grinding:** scraping along a wall, barrier or car at speed barely slows
  or damages your car, throws sparks, and is never a crash (`@grind`,
  `@grind_speed`, `@grind_drag`, `@grind_damage`, `@grind_sparks`).
- **Prop hits can't spin the car:** for a moment it keeps turning the way it
  was, so going straight stays straight and a drift carries on
  (`@prop_straight`).
- **Tyres sink into kerbs** instead of the whole car being lifted over them,
  which smooths out handling at speed (`@tyres_clip` *(setup)*).
- The camera no longer zooms in when a knocked prop passes through the car.

## Traffic

- **Burnout's traffic lights work:** the 135 light-controlled junctions
  cycle on Burnout's timings, cars stop at red, queue and go on green, and
  the overhead lights with no GTA model light up (`@junctions`,
  `@light_time`, `@light_lamps`, `@light_hold`).
- **Give way at junctions without lights:** cars wait while another crosses
  their path (`@give_way`).
- **No more gridlock:** a car stuck in a junction for 10 s out of your sight
  is removed (`@unjam`).

## Map

- **Distant scenery:** Burnout's background mountains and its low-detail
  model of every part of the city, so the skyline and hills show from far
  away and swap to full detail as you get closer.
- **Builds from Burnout with the OTN mod installed:** its changed and new
  areas, props and textures convert too, and generated collision that would
  clash with OTN's new pieces is left out.
- **Texture fixes:** road markings on tunnel, Big Surf and bridge roads,
  white lawns and planters on OTN, and power and tram cables are black
  instead of white.
- 572 more flooded water squares removed (2,667 in all), 16 more collision
  fixes (940), 8 more sidewalk fixes and 6 more parked vehicles.

## Setup

- **Map choice:** Paradise City, or GTA's own Los Santos with only the
  Burnout driving. Run the setup again and pick the other map to switch
  (`@mode` now shows which is installed; `@mode off` no longer gives
  normal GTA back).
- New steps for the radar and map, the drive-thrus and the junkyard's car
  list. A Burnout folder with a mod installed in it is detected and rebuilt.
- The map viewer and editors are no longer in this repository: they're now
  a separate desktop app, Paradise Viewer.

## Upgrading from v0.2.1

Run the new setup over your install. Your settings keep their values and
the new ones are added with their defaults. Most of the conversion runs
again, so expect about as long as a first install.

---

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
