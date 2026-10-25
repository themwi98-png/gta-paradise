# GTA Paradise — what's in the mod

Burnout Paradise City (from Burnout Paradise Remastered) replaces Los Santos in
GTA V **Legacy** story mode, with GTA's traffic, pedestrians and police living in
it and Burnout-style driving on top.

**Status key:** ✅ confirmed working in game · 🟡 needs further testing · ⬜ not done yet / idea

Last updated: 2026-10-25 (v0.3.1)


## The map

| Feature | Status | Notes |
|---|---|---|
| Paradise City buildings, roads and scenery (models + textures) | ✅ | Every Burnout map section converted to GTA models and map files (`bpx gta-visual`). |
| Collision (drive / walk on everything) | 🟡 | Burnout's own collision (the player's `WORLDCOL.BIN`) plus about 441,000 added out-of-bounds pieces, kept separately in `tools/collision/paradise_extra.pcx` (only the added pieces, so no Burnout data is shared), repaired for GTA. `bpx col-split` makes the `.pcx` from a combined collision file. |
| Los Santos removed | ✅ | Base map, Rockstar's patch packs, scenario points and stray grass all removed. |
| Map height | ✅ | Placed with `--offset 0,0,9.9` so Paradise's sea sits at GTA's sea level. |
| Sea | 🟡 | Sea level tuned by eye; covers the whole world, no gaps; calm water near shore, no waves through streets. |
| Lakes (incl. Silver Lake) and river | 🟡 | Raised lake levels from Burnout's water meshes / water collision; shoreline follows curves. |
| Water fixes | ✅ | 2,667 flooded 16 m squares removed with the water editor (`tools/edits/water_edits.json`). |
| Street props | ✅ | 16,120 of Burnout's 27,512 props replaced by similar GTA props: lamp posts, traffic lights, rail crossing signals, roadworks barriers, cones, water barrels, bins, dumpsters, benches, phones, newspaper boxes, mailboxes, parking meters, picnic tables, drums, crates, stop / yield / no-entry signs. Same spots and facing, set on the ground. Table: `tools/props_map.csv`. |
| Breakable barriers instead of fixed ones | ✅ | All roadworks / stunt barriers now use the better-looking breakable `prop_barrier_work06a` (marks 14/15). |
| Movable boxes instead of fixed ones | ✅ | Big crates and box pallets now use the movable `prop_box_wood01a` (marks 10/11/16). |
| Barrels sitting on the ground | ✅ | GTA models centred on their middle (barrels) are lifted so they no longer sink halfway (mark 12). |
| Swinging stunt gates removed | ✅ | Burnout's stunt gates (`STU_gate01`) are left out entirely: in Burnout they break once and stay gone, so in a normal playthrough they're usually missing anyway. |
| Railings, overhead lane-sign gantries, warning chevrons | ⬜ | No close GTA match picked yet (railings are the biggest gap: 2,860). |
| District name signs, billboards | ⬜ | Paradise-specific; no GTA equivalent. |
| Radar and pause map showing Paradise City | ✅ | Roads, buildings, railways, lakes and sea drawn from Burnout's map into GTA's radar tiles and pause-map images (`bpx gta-minimap`, the setup's "Radar and map" step). The coast is hand-traced in the map coast editor (`tools/edits/minimap_edits.json`). District names are still Los Santos's. |
| GPS routes | 🟡 | A waypoint gets a route over Paradise's roads, found by the plugin (GTA's own route finder doesn't work on the converted roads) and drawn on the radar; it reroutes when you leave it. |
| Distant scenery and low-detail city | ✅ | Burnout's background mountains are always drawn, and its low-detail model of each map section stands in for that section until you're close (`bpx gta-visual`, on by default; `--lod-far off` leaves them out). |
| Burnout with the OTN mod | ✅ | The setup builds from a Burnout folder with OTN installed: its changed and new map sections, props and textures convert, and generated collision that would clash with OTN's new pieces is left out (`tools/collision/modded_maps.json`). |
| Known collision spots (2) | ⬜ | Phantom slope at (1421, 759), hovering under the railway at (2615, 676) — `tools/collision_issues.md`. Saved for a full collision pass. |

## Life in the city

| Feature | Status | Notes |
|---|---|---|
| Traffic | ✅ | GTA cars drive Burnout's road network, converted to two-way GTA roads (every road reachable from every other). How busy each road is follows Burnout's own traffic flow per lane: busy downtown, quiet country roads. `@traffic` sets the overall amount (0 = empty roads, up to 3). |
| Manual traffic fixes | ✅ (tool) | Viewer traffic editor (reverse / delete / shift lanes, set speed (shown with GTA's speed band), set traffic level, connect, draw new lanes; colour by traffic, select an area; parked vehicles: move, turn, add, copy / paste, set to any vehicle model). |
| Intersections marked for the AI | ✅ | Junction and dead-end markers like Rockstar's, so AI turns and yields properly. |
| Parked cars | ✅ | Burnout's 952 parked cars as GTA parked-car spawners, on the ground, plus hand-placed ones. Any spot can be set to a specific vehicle (planes, helicopters, boats, ATVs...), which then always spawns there. |
| Pedestrians | ✅ | Walkable navmesh generated for the whole city (pavements, plazas, island). |
| Districts / police type | ✅ | Burnout's districts (hand-aligned) as GTA zones: LSPD everywhere, peds on the island. |
| Los Santos leftover spawns removed | 🟡 | NPCs and vehicles that appear in mid-air (over 8 m up, or over nothing) are removed as they spawn, near or far (flying aircraft excepted). GTA's trains, whose tracks now run through the sky, are switched off. Logged to `paradise_spawns.csv`. |
| No-spawn zones | 🟡 | Placed in the viewer's Spawns panel; GTA's parked-car spawners and NPC spawn spots are switched off inside, and anything else that appears there is removed. One zone so far (mark 21). |
| Traffic lights that cycle | 🟡 | Burnout's 135 light-controlled junctions cycle on its timings (10 s green), GTA's traffic-light props show the colour, and the overhead lights with no GTA model light up. Cars stop at red, queue, and go on green one after another; at junctions without lights they give way to crossing cars. A car stuck in a junction for 10 s out of your sight is removed (`@junctions`, `@light_time`, `@light_lamps`, `@give_way`, `@unjam`, `@light_hold`). |
| Pedestrian navmesh hand-editing | 🟡 | Implemented, untested |

## Police

| Feature | Status | Notes |
|---|---|---|
| Police chase on Paradise roads | 🟡 | All of Paradise's road data kept loaded, so police dispatched from far away can route to you. Needs further testing. |
| Faster, more aggressive pursuers | ✅ | Top skill and aggression, GTA's engine power, +100 % top speed (`@police_power`, `@police_top`). |
| Police survive their own crashes | 🟡 | Pursuing police cars can't be damaged until your car hits them; they don't catch fire, their engines keep running, and their drivers aren't killed or thrown out (`@police_tough`). |
| Police stay in their cars while you drive | ✅ | They still get out if you're on foot. |
| Police know where you are | 🟡 | Any police car (or helicopter) within 400 m knows where you are, seen or not; get further away to lose them (`@police_sight`). |
| Police keep up | ✅ | Police cars go where they point instead of sliding out (`@police_grip`), get extra pull when speeding up (`@police_accel`) and brake 3× harder (`@police_brakes`). Flipped or stuck ones are put back on the nearest road, facing you. |
| Police ram you | ✅ | Police cars near you go for a PIT from behind or a slam from alongside, and oncoming ones steer in for a head-on (`@police_ram`, `@police_ram_oncoming`, `@police_rammers`). |
| Police follow you off the roads | 🟡 | Police route along traffic lanes only. When you drive more than 40 m from any lane, police cars within 450 m are sent straight at you (by road until within 250 m), and go back to normal pursuit once you're back on the roads. Roads Burnout has no lanes on can be given lanes with the viewer's traffic editor. |
| Chase recorder | ✅ (diagnostic) | `paradise_chase.csv` in the GTA folder logs pursuers once a second while wanted. |

## Driving — Burnout style

| Feature | Status | Notes |
|---|---|---|
| Much more crash damage on all cars | ✅ | All 791 vehicles (base game + DLC): deep dents on every car (trucks no longer tear apart), body damage 5×, engine damage half GTA's, so battered cars keep driving (`HANDLING` in `crates/bpx/src/gtadata.rs`). |
| Faster cars, stronger brakes | ✅ | Top speed 1.35×, engine power 1.35× (live: `@car_top_speed`, `@car_power`), brakes 2.5× (`@car_brakes`, built in by the setup; every vehicle, police included). |
| Grip | ✅ | Cornering and sliding grip 1.2× (`@car_grip`, built in by the setup), wheelspin cut to 0.6×. |
| Heavier jumps | ✅ | Cars in the air fall at 3× gravity (`@air_gravity`). |
| Steady flight | ✅ | In the air the car keeps its nose the way it's flying (following the arc partway) and its wheels down, instead of pitching or spinning (`@air_control`). |
| Barrel rolls | ✅ | Leaving a split ramp rolling (either way), the car turns exactly once and lands on its wheels. Straight jumps don't count (`@barrel_roll`, `@barrel_roll_trigger`). |
| Downforce | ✅ | Fast cars on the ground are pressed down harder, growing with speed squared: +0.75 g at ~180 km/h, at most +3 g (`@downforce`). |
| Drift assist | ✅ | Holds a slide at a sensible angle and softens the catch so it doesn't flick into a spin (`@drift_assist`, `@drift_angle`). |
| Brake to drift | 🟡 | Tapping the brake while turning (steering at least `@brake_drift_steer`, or already swinging round) works as the handbrake for a moment, then as the normal brake if held. Going straight it's always the brake. With the handbrake already held, or pressed during it, it's the normal brake (`@brake_drift`, `@brake_drift_time`, `@brake_drift_speed`). |
| Off-road as fast as the road | ✅ | No ground type (sand, dirt, gravel, grass...) slows the car or caps its top speed; grip still varies. |
| Extra dents and smashed glass on hard hits | ✅ | From about 40 km/h of sudden speed loss (`@impact_speed`). |
| **Wrecks** | ✅ | Hard crash (≈90 km/h sudden speed loss, `@wreck_speed`): car crumpled all round, glass and panels gone, explodes, 5 s slow-motion crash cam with "WRECKED" (GTA's own lettering and sound, colours drained a little, `@wreck_desat`), then you respawn on the nearest road in a fresh copy of the same car (colours, upgrades, plate), already rolling. No penalty. |
| Slow motion starts just before the impact | ✅ | Only when a wreck is all but certain: lines traced straight ahead along your path, within the car's width, must agree (2 of 3) on a near head-on hit at least 25 % over the wreck threshold within 0.12 s. Steering clear brings normal speed back and shows a yellow "DRIVEAWAY" for 2 s (`@precrash`, `@precrash_lead`). |
| Crash camera on big hits | ✅ | Any hit of at least 72 km/h sudden speed loss shows the slow-motion crash camera from the impact for 1.5 s; if it wasn't a wreck, DRIVEAWAY (`@crash_cam_speed`, `@crash_cam_hold`). |
| Wreck scene cleared | ✅ | Once the screen fades after a wreck, NPCs, vehicles and fires within 6 m of the impact point and of where the wreck ended up are removed, and again around your new car just before the screen comes back. Vehicles and people within 10 m at the impact, or touched by the wreck as it tumbles, are removed wherever they were thrown (`@wreck_clear`; 0 = off). |
| Wreck when the car is finished | ✅ | On fire, engine dead or below 100 health (`@wreck_engine`), undrivable, or jammed (wheels down, accelerator held, not moving) for 2.5 s also counts as a wreck. A wheel knocked out of line can count too (`@wreck_wheel`, off by default). Being upside down never does: hold steering to roll the car back over (`@wreck_damage`, `@wreck_stuck`). Your tyres can't burst and your axles don't bend from knocks (`@strong_tyres`, `@strong_axles`). |
| Flying through the windscreen only in wrecks | 🟡 | GTA's own windscreen ejection is on only in the run-up to a certain wreck; being thrown out is always a wreck, and the crash camera frames car and driver. Normal crashes and prop hits never throw you out, and drivers can't die in a crash (`@wreck_eject`). |
| **Boost** | ✅ | Hold A (controller) / X (keyboard): exhaust flames, extra engine power and boost sound; up to ~1.8× the car's top speed (105 mph to about 300 mph). Gauge bottom right, lasts 15 s. Locked when empty until you let go. |
| Earning boost | ✅ | Driving fast, oncoming lane / wrong way, near misses (a car passing within about 2.5 m; oncoming worth more), drifts, air time and big air. |
| Props don't hurt or slow your car | ✅ | Near a placed prop the car can't be damaged or dented, and hitting one never wrecks you. Props never change the car's speed, direction or spin: the moment one touches the car, its motion is put back to just before (not while touching another vehicle). |
| Vehicles always hurt | 🟡 | Props never hurt the car (100 %). The protection drops just before another vehicle hits you (only vehicles closing in, not traffic alongside), and a hit while touching a vehicle is always a real crash. |
| Props get tossed | ✅ | A prop you hit passes through the car as it's thrown clear (up and over, or well out to the side for lamp posts and benches); falling debris bounces off the car. The camera ignores props passing through. |
| Nothing gets stuck under the car | 🟡 | While moving, any loose prop or broken piece just ahead of the nose, along the way the car is moving (further ahead the faster you go), is swept aside. GTA doesn't expose mass, so heavy props are picked by model. |
| **Takedowns** | ✅ | A car you hit stays vulnerable for `@takedown_window` (2 s): if it crashes, rolls or spins out, it's your takedown (banner: SLAM / SHUNT / T-BONE / VERTICAL / ROADBLOCK, DOUBLE / TRIPLE..., +half a boost gauge, `@takedown_boost`). A hit with 20 m/s closing speed takes it down at once (`@takedown_smash`), parked roadblock cars too. Taken-down cars are thrown off your path and explode (`@takedown_launch`, `@takedown_explode`). Any vehicle counts (`@takedown_any`). |
| Taken down | ✅ | A wreck just after a police car hit you is TAKEN DOWN and costs a quarter of the gauge (`@taken_down_boost`). The police car that did it is spared by the wreck clear-up. |
| Escaping by takedowns | ✅ | Five police takedowns in one chase: YOU ESCAPED!, wanted level cleared (`@chase_takedowns`). |
| Ramming without spinning | ✅ | While ramming, your car can't be spun: it keeps pointing and moving the way it was going, steering still turns it, and the ram's slowdown stays (`@takedown_straight`, `@takedown_keep`). Rams from behind or alongside never wreck you. |
| Grinding | ✅ | Scraping along a wall, barrier or car above 12 m/s barely slows (5 % a second) or damages (10 %) your car, throws GTA's rim sparks, and is never a crash (`@grind`, `@grind_speed`, `@grind_drag`, `@grind_damage`, `@grind_sparks`). |
| Prop hits don't spin the car | 🟡 | For 0.2 s after a prop hit the car keeps turning the way it was: straight stays straight, a drift carries on (`@prop_straight`). |
| Tyres sink into kerbs | ✅ | Every vehicle's tyres may sink into kerbs and bumps instead of the whole car being lifted over them (`@tyres_clip`, built in by the setup). |

## Around Paradise City

| Feature | Status | Notes |
|---|---|---|
| Drive-thrus with map blips | ✅ | Burnout's own drive-thru boxes: Auto Repair fixes the car, Gas Station fills the boost gauge, Paint Shop resprays it in random colours, Junkyard opens the car menu (`@shops`). |
| Junkyard: modify the car | ✅ | Stops you dead on entry. Respray (19 colours plus factory, finish), rim type and rims, every body and performance part the car has, window tint, plate, livery, turbo, xenons. Not while wanted. |
| Junkyard: select a new car | ✅ | Every GTA road vehicle in your game (776 in the current Legacy version), by class. The highlighted car turns on a turntable in the junkyard's display lot (marked by hand, `tools/edits/junkyard_lots.csv`), with bars for top speed, acceleration, braking and traction; Enter drives it away. Wrecks respawn you in the new car. |
| GTA's missions removed | ✅ | Story missions, side activities, random events and their map blips are stopped and hidden (`@gta_missions` off). |

## Player experience

| Feature | Status | Notes |
|---|---|---|
| Straight into Paradise on load | ✅ | Screen stays black through GTA's arrival scene (e.g. Vespucci pull-ups); you appear at `@start`, in a fresh `@start_car` (a Drift Tampa unless set otherwise). |
| Skip the landing menu | N/A | Not modded: use GTA's own startup option (Steam launch options / in-game landing page setting) to go straight to Story Mode. |
| Wasted → respawn nearby | ✅ | GTA's normal "wasted" plays (with its penalties), then you appear on foot near where you died instead of a Los Santos hospital. The time of day stays where it was. |
| Busted → respawn nearby | 🟡 | Same for arrests (penalties kept). |
| Rescue from falling through the map | ✅ | 1.5 s with nothing below (not swimming / flying / boating) puts you back near the last place you had ground under you. |
| Swimming and boats | ✅ | Water no longer triggers the rescue. |

### Keys (in game)

| Key | What it does |
|---|---|
| A / X (in a car) | Boost |
| F9 | Mark this spot (saved to `paradise_marks.csv` so problems can be found exactly) |
| F6 | Teleport to the next destination listed in `paradise.ini` |
| F5 | Release a teleport hold |
| F4 | Show collision overlay |
| F7 | Hide the plugin's text |
| F10 | Re-apply the Paradise map (debug) |

---

## Editing tools (Paradise Viewer)

Step-by-step instructions for every editor: [HOW_TO.md](HOW_TO.md).

The map viewer and its editors are a separate desktop app, Paradise Viewer
(its own project, no longer in this repository). It shows the edits saved in
this mod's `tools/edits/` and exports new ones to your Downloads folder.

| Tool | Status | Saves to | Apply with |
|---|---|---|---|
| Collision editor (delete generated pieces; native pieces too with "Include native collision") | ✅ / 🟡 native | `tools/edits/collision_edits.json` | `tools/rebuild_collision.sh` |
| Traffic editor (lanes and parked cars) | ✅ | `tools/edits/traffic_edits.json` | `tools/rebuild_traffic.sh` |
| Water editor (delete flooded squares) | ✅ | `tools/edits/water_edits.json` | `tools/rebuild_water.sh` |
| Spawn editor (no-spawn zones; shows F9 marks and removed spawns) | 🟡 | `tools/edits/spawn_blocks.json` | `tools/install_dlc.sh` (run `tools/sync_from_game.sh` first to see new marks / removals) |
| District aligner | ✅ | (values pasted to Claude) | — |
| Prop replacement table | ✅ | `tools/props_map.csv` (text file) | `tools/rebuild_props.sh` |
| Sidewalk editor (draw shapes to remove / add sidewalk for pedestrians) | 🟡 | `tools/edits/sidewalk_edits.json` | `tools/rebuild_navmesh.sh` |
| Map coast editor (draw land / sea shapes over the radar's coast) | ✅ | `tools/edits/minimap_edits.json` | the setup's "Radar and map" step |
| Junctions editor (switch junctions off, resize, add) | 🟡 | `tools/edits/junction_edits.json` | `tools/rebuild_traffic.sh` |
| Junkyard display lots (F9 marks in game) | ✅ | `tools/edits/junkyard_lots.csv` (text file) | the setup's drive-thrus step |

Each editor's export goes in `tools/edits/`, and the editors load those files
on start, so saved edits always show and survive rebuilds.

## Build scripts (`tools/`, run with GTA closed)

| Script | Rebuilds |
|---|---|
| `build_dlc.sh` | Everything (models, collision, roads, peds, props, water) |
| `rebuild_collision.sh` | Collision, water, navmesh |
| `rebuild_traffic.sh` | Roads, parked cars, props |
| `rebuild_props.sh` | Props |
| `rebuild_water.sh` | Water |
| `rebuild_navmesh.sh` | Pedestrian navmesh (sidewalk edits) |
| `sync_from_game.sh` | Copies F9 marks and removed-spawn log from the game for the viewer |
| `build_crash.sh` | Car handling (damage, speed, grip) |
| `install_dlc.sh` | Re-installs what's built (also plugin and `paradise.ini`) |

## Settings (`paradise.ini` in the GTA folder)

Re-read every few seconds while playing. Running the setup again keeps the values you
changed (untouched ones move to new defaults, new settings arrive with theirs); the development install
script overwrites it with the project's copy.

| Setting | Now | Meaning |
|---|---|---|
| `@mode` | paradise | Which map is installed: `paradise` or `los_santos` (set by the setup's map choice) |
| `@start` | 994.9 862.7 25.2 | Start / rescue spot |
| `@start_car` | drifttampa | Vehicle you start in (`none` = on foot) |
| `@shops` | on | Burnout's drive-thrus (repair, gas, paint, junkyard) |
| `@gta_missions` | off | Keep GTA's missions, side activities and their blips |
| `@wreck` | on | Burnout wrecks |
| `@wreck_speed` | 25 | Sudden speed loss (m/s) that wrecks |
| `@impact_speed` | 11 | Sudden speed loss for extra dents |
| `@wreck_explode` | on | Car blows up in a wreck |
| `@wreck_eject` | on | Fly through the windscreen in a wreck (never otherwise) |
| `@respawn_speed` | 12 | Speed of the fresh car (m/s) |
| `@crash_time` | 5 | Slow-motion crash seconds |
| `@slowmo` | 0.15 | Game speed during a wreck |
| `@crash_cam` | on | Orbiting crash camera |
| `@precrash` | on | Start the slow-mo crash camera just before a wreck |
| `@precrash_lead` | 0.12 | How far ahead (game seconds) a wreck is seen coming |
| `@crash_cam_speed`, `@crash_cam_hold` | 20, 1.5 | Hits this hard (m/s) show the crash camera for this long (s) |
| `@wreck_desat` | 0.6 | Colour drained during a wreck (0 = none, 1 = black and white) |
| `@wreck_damage` | on | Wreck when on fire / dead / jammed |
| `@wreck_stuck` | 2.5 | Seconds jammed before a wreck |
| `@wreck_engine` | 100 | Engine health below which the car is wrecked |
| `@wreck_wheel` | 0 | Wreck when a wheel is this many cm out of line (0 = off) |
| `@strong_axles`, `@strong_tyres` | on, on | Axles don't bend from knocks; tyres can't burst |
| `@perf` | on | Log the plugin's cost per frame to `paradise.log` every 15 s |
| `@debug_log` | off | Old position probe and driving trace (debugging) |
| `@prop_keep` | 1 | Speed kept on a prop hit the plugin couldn't undo (fallback) |
| `@prop_straight` | on | A prop hit can't spin the car |
| `@wreck_crush`, `@wreck_crush_rounds` | 8000, 8 | How mangled wrecked cars get |
| `@wreck_clear` | 6 | Radius (m) cleared of NPCs, vehicles and fires after a wreck (0 = off) |
| `@police_power`, `@police_top` | 1, 100 | Pursuing police engine power, extra top speed % |
| `@police_tough` | on | Police cars can't be damaged until you hit them, and survive their own crashes |
| `@police_grip` | 1 | Share of police cars' sideways slide taken away (0 = GTA's physics) |
| `@police_sight` | 400 | Police this close (m) know where you are (0 = only what they see) |
| `@police_accel`, `@police_brakes` | 1, 3 | Police extra pull (g) when speeding up; braking, times GTA's |
| `@police_ram`, `@police_ram_oncoming`, `@police_rammers` | 12, on, 4 | Police ram you this much faster than you (m/s, 0 = off); oncoming ones go head-on; how many at once |
| `@takedowns` | on | Takedowns |
| `@takedown_window`, `@takedown_speed` | 2, 10 | How long a hit car stays vulnerable (s); speed change (m/s) that counts as its crash |
| `@takedown_smash` | 20 | Closing speed (m/s) that takes a car down at once (0 = never) |
| `@takedown_boost`, `@taken_down_boost` | 0.5, 0.25 | Gauge gained per takedown; lost when taken down |
| `@takedown_any` | on | Any vehicle can be taken down |
| `@takedown_explode`, `@takedown_launch` | on, 14 | Taken-down cars blow up; thrown off your path at this speed (m/s) |
| `@chase_takedowns` | 5 | Police takedowns that end a chase (0 = never) |
| `@takedown_keep`, `@takedown_straight` | 0.9, on | Speed your car keeps when ramming; held straight while ramming |
| `@grind`, `@grind_speed` | on, 12 | Grinding; slowest speed that grinds (m/s) |
| `@grind_drag`, `@grind_damage`, `@grind_sparks` | 0.05, 0.1, 1 | Speed lost per second; share of damage taken; spark size |
| `@traffic` | 1 | Amount of traffic (0 = empty roads, up to 3) |
| `@junctions`, `@light_time`, `@light_lamps` | on, 1, on | Traffic lights and give way; light timings, times Burnout's; lamps on overhead lights without a GTA model |
| `@give_way`, `@unjam`, `@light_hold` | on, 10, halt | Give way at unlit junctions; remove cars stuck this long (s, 0 = never); stop at red by braking (`halt`) or cutting top speed (`cap`) |
| `@car_top_speed`, `@car_power` | 1.35, 1.35 | Every car's top speed and engine power, times GTA's |
| `@car_grip`, `@car_brakes` | 1.2, 2.5 | Grip and braking, times GTA's (built in: re-run the setup after changing) |
| `@tyres_clip` | on | Tyres sink into kerbs (built in: re-run the setup after changing) |
| `@air_gravity` | 3 | Gravity on cars in the air |
| `@air_control` | 0.7 | How steady cars stay in the air (0 = off, 1 = held) |
| `@barrel_roll`, `@barrel_roll_trigger` | on, 90 | Barrel rolls off split ramps; how hard a ramp must roll you (deg/s) |
| `@downforce` | 0.75 | Extra g on fast cars at ~180 km/h |
| `@drift_assist`, `@drift_angle` | 1.1, 45 | Drift help (0 = off, up to 3); largest drift angle |
| `@brake_drift`, `@brake_drift_time`, `@brake_drift_speed` | on, 0.5, 2.2 | Brake to drift; how long a press is the handbrake (s); from this speed (m/s) |
| `@brake_drift_steer` | 0.3 | Only while steering this much (0 to 1; 0 = going straight too) |
| `@boost` | on | Boost |
| `@boost_accel` | 9 | Boost acceleration (m/s²) |
| `@boost_top`, `@boost_min_top`, `@boost_max_top` | 1.8, 47, 135 | Boosted top speed (× car top, min/max m/s) |
| `@boost_seconds` | 15 | Full gauge duration |
| `@boost_fill` | 1.5 | Gauge refill rate |

---

## Ideas / not started

- More Burnout mechanics (boost types, events)
- District names on the map
- Railings, sign gantries, chevrons as GTA props
- Full collision pass (known spots above)
- Pedestrian navmesh hand-editing beyond sidewalks (blocking / adding walkable areas)
- Water editor: add water / change lake levels
