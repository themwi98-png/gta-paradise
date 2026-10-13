# GTA Paradise — what's in the mod

Burnout Paradise City (from Burnout Paradise Remastered) replaces Los Santos in
GTA V **Legacy** story mode, with GTA's traffic, pedestrians and police living in
it and Burnout-style driving on top.

**Status key:** ✅ confirmed working in game · 🟡 installed, not yet confirmed in
game · ⬜ not done yet / idea

Last updated: 2026-10-12

---

## Ground rules

- Only the **Legacy** install (`Grand Theft Auto V`, GTA 1.0.3889) is modded,
  through the OpenIV `mods` folder. **GTA V Enhanced (used for Online) is never
  touched.** Story mode only.
- The original `update/update.rpf` is untouched (backup: `update.rpf.bpbackup`);
  everything goes in `mods/update/update.rpf` and `mods/update/x64/dlcpacks/`.
- GTA must be closed to install (the install script refuses while it runs).
- `@mode off` in `paradise.ini` gives back normal GTA.

---

## The map

| Feature | Status | Notes |
|---|---|---|
| Paradise City buildings, roads and scenery (models + textures) | ✅ | Every Burnout map section converted to GTA models and map files (`bpx gta-visual`). |
| Collision (drive / walk on everything) | ✅ | Burnout's own collision (the player's `WORLDCOL.BIN`) plus about 441,000 added out-of-bounds pieces, kept separately in `tools/collision/paradise_extra.pcx` (only the added pieces, so no Burnout data is shared), repaired for GTA. `bpx col-split` makes the `.pcx` from a combined collision file. |
| Hand-made collision fixes | ✅ | 654 pieces deleted with the viewer's collision editor (`tools/edits/collision_edits.json`). |
| Los Santos removed | ✅ | Base map, Rockstar's patch packs, scenario points and stray grass all removed. |
| Map height | ✅ | Placed with `--offset 0,0,9.9` so Paradise's sea sits at GTA's sea level. |
| Sea | ✅ | Sea level tuned by eye; covers the whole world, no gaps; calm water near shore, no waves through streets. |
| Lakes (incl. Silver Lake) and river | ✅ | Raised lake levels from Burnout's water meshes / water collision; shoreline follows curves. |
| Hand-made water fixes | ✅ | 656 flooded 16 m squares removed with the viewer's water editor (`tools/edits/water_edits.json`). |
| Street props | ✅ | 16,120 of Burnout's 27,512 props replaced by similar GTA props: lamp posts, traffic lights, rail crossing signals, roadworks barriers, cones, water barrels, bins, dumpsters, benches, phones, newspaper boxes, mailboxes, parking meters, picnic tables, drums, crates, stop / yield / no-entry signs. Same spots and facing, set on the ground. Table: `tools/props_map.csv`. |
| Breakable barriers instead of fixed ones | 🟡 | All roadworks / stunt barriers now use the better-looking breakable `prop_barrier_work06a` (marks 14/15). |
| Movable boxes instead of fixed ones | 🟡 | Big crates and box pallets now use the movable `prop_box_wood01a` (marks 10/11/16). |
| Barrels sitting on the ground | 🟡 | GTA models centred on their middle (barrels) are lifted so they no longer sink halfway (mark 12). |
| Swinging stunt gates removed | 🟡 | Burnout's stunt gates (`STU_gate01`) are left out entirely: in Burnout they break once and stay gone, so in a normal playthrough they're usually missing anyway. |
| Railings, overhead lane-sign gantries, warning chevrons | ⬜ | No close GTA match picked yet (railings are the biggest gap: 2,860). |
| District name signs, billboards | ⬜ | Paradise-specific; no GTA equivalent. |
| Minimap / pause map showing Paradise City | ⬜ | Still shows Los Santos art (GPS routes use Paradise roads). Idea: render a top-down Paradise map into GTA's radar. On hold. |
| Distant LODs / horizon | ⬜ | |
| Known collision spots (2) | ⬜ | Phantom slope at (1421, 759), hovering under the railway at (2615, 676) — `tools/collision_issues.md`. Saved for a full collision pass. |

## Life in the city

| Feature | Status | Notes |
|---|---|---|
| Traffic | ✅ | GTA cars drive Burnout's road network, converted to two-way GTA roads (every road reachable from every other). |
| Hand-made traffic fixes | ✅ (tool) | Viewer traffic editor (reverse / delete / shift lanes, set speed, connect, draw new lanes, edit parked cars). No edits made yet. |
| Intersections marked for the AI | ✅ | Junction and dead-end markers like Rockstar's, so AI turns and yields properly. |
| Parked cars | ✅ | Burnout's 952 parked cars as GTA parked-car spawners, on the ground. |
| Pedestrians | ✅ | Walkable navmesh generated for the whole city (pavements, plazas, island). |
| Districts / police type | ✅ | Burnout's districts (hand-aligned) as GTA zones: LSPD everywhere, peds on the island. |
| Los Santos leftover spawns removed | 🟡 | NPCs and vehicles that appear in mid-air (over 8 m up, or over nothing) are removed as they spawn, near or far (flying aircraft excepted). GTA's trains, whose tracks now run through the sky, are switched off. Logged to `paradise_spawns.csv`. |
| No-spawn zones | 🟡 | Placed in the viewer's Spawns panel; GTA's parked-car spawners and NPC spawn spots are switched off inside, and anything else that appears there is removed. One zone so far (mark 21). |
| Traffic lights that cycle | ⬜ | GTA may or may not drive the placed traffic-light props; Burnout has none either. |
| Pedestrian navmesh hand-editing | ⬜ | Wanted later. |

## Police

| Feature | Status | Notes |
|---|---|---|
| Police chase on Paradise roads | ✅ | All of Paradise's road data kept loaded, so police dispatched from far away can route to you. |
| Faster, more aggressive pursuers | ✅ | Top skill and aggression, 2× engine power, +50 % top speed (`@police_power`, `@police_top`). |
| Police stay in their cars while you drive | ✅ | They still get out if you're on foot. |
| Police notice you when in plain sight | ✅ | A police car within 120 m with a clear view reports you. |
| Police follow you off the roads | 🟡 | Police route along traffic lanes only. When you drive more than 40 m from any lane, police cars within 450 m are sent straight at you (by road until within 250 m), and go back to normal pursuit once you're back on the roads. Roads Burnout has no lanes on can be given lanes with the viewer's traffic editor. |
| Chase recorder | ✅ (diagnostic) | `paradise_chase.csv` in the GTA folder logs pursuers once a second while wanted. |

## Driving — Burnout style

| Feature | Status | Notes |
|---|---|---|
| Much more crash damage on all cars | ✅ | All 791 vehicles (base game + DLC): dents 6×, body damage 4×, engine damage 3× (`HANDLING` in `crates/bpx/src/gtadata.rs`). |
| Faster cars | ✅ | Top speed 1.35×, engine power 1.5×, brakes 1.25×. |
| More grip | 🟡 | Cornering and sliding grip 1.4× (was 1.2×), wheelspin cut to 0.6×. |
| Heavier jumps | ✅ | Cars in the air fall at 3× gravity (`@air_gravity`). |
| Downforce | 🟡 | Fast cars on the ground are pressed down harder, growing with speed squared: +1 g at ~180 km/h, at most +3 g (`@downforce`). |
| Extra dents and smashed glass on hard hits | ✅ | From about 40 km/h of sudden speed loss (`@impact_speed`). |
| **Wrecks** | ✅ | Hard crash (≈65 km/h sudden speed loss, `@wreck_speed`): car crumpled all round, glass and panels gone, explodes, 5 s slow-motion crash cam with "WRECKED", then you respawn on the nearest road in a fresh copy of the same car (colours, upgrades, plate), already rolling. No penalty. |
| Slow motion starts just before the impact | 🟡 | Only when a wreck is all but certain: lines traced straight ahead along your path, within the car's width, must agree (2 of 3) on a near head-on hit at least 25 % over the wreck threshold within 0.12 s. Otherwise slow-mo starts at the impact. Steering clear or a non-wreck hit brings normal speed back and shows a yellow "DRIVEAWAY" for 2 s (`@precrash`, `@precrash_lead`). |
| Wreck scene cleared | 🟡 | Once the screen fades after a wreck, NPCs, vehicles and fires within 6 m of the impact point and of where the wreck ended up are removed, and again around your new car just before the screen comes back. Vehicles and people within 10 m at the impact, or touched by the wreck as it tumbles, are removed wherever they were thrown (`@wreck_clear`; 0 = off). |
| Wreck when the car is finished | 🟡 | On fire, engine dead, undrivable, or jammed (wheels down, accelerator held, not moving) for 2.5 s also counts as a wreck. Being upside down never does: hold steering to roll the car back over (`@wreck_damage`, `@wreck_stuck`). |
| No flying through the windscreen | ✅ | |
| **Boost** | ✅ | Hold A (controller) / X (keyboard): exhaust flames, extra engine power and boost sound; up to ~1.8× the car's top speed (105–210 mph). Gauge bottom right, lasts 7 s. Locked when empty until you let go. |
| Earning boost | ✅ | Driving fast, oncoming lane / wrong way, near misses (a car passing within about 2.5 m; oncoming worth more), drifts, air time and big air. |
| Props don't hurt your car | ✅ | Near a placed prop the car can't be damaged or dented; hitting one never wrecks you and keeps 90 % of your speed (`@prop_keep`). |
| Vehicles always hurt | 🟡 | Props never hurt the car (100 %). The protection drops just before another vehicle hits you (only vehicles closing in, not traffic alongside), and a hit while touching a vehicle is always a real crash. |
| Props get tossed | ✅ | A prop you hit is thrown up and over the car whatever its weight (benches etc. fly like boxes). |
| Nothing gets stuck under the car | 🟡 | While moving, any loose prop or broken piece just ahead of the nose (further ahead the faster you go) is swept aside, away from the car's centre line (dead ahead: up and over). Lamp posts and benches are thrown much harder: well out to the side, or high over the car and off to one side, both when swept and when knocked over by the car (they stand fixed until hit). GTA doesn't expose mass, so heavy props are picked by model. |
| Takedowns | ⬜ | Burnout mechanic, not started. |

## Player experience

| Feature | Status | Notes |
|---|---|---|
| Straight into Paradise on load | ✅ | Screen stays black through GTA's arrival scene (e.g. Vespucci pull-ups); you appear at `@start`. |
| Skip the landing menu | ⬜ (setting) | Not modded: use GTA's own startup option (Steam launch options / in-game landing page setting) to go straight to Story Mode. |
| Wasted → respawn nearby | ✅ | GTA's normal "wasted" plays (with its penalties), then you appear on foot near where you died instead of a Los Santos hospital. |
| Busted → respawn nearby | 🟡 | Same for arrests (penalties kept). |
| Rescue from falling through the map | ✅ | 1.5 s with nothing below (not swimming / flying / boating) puts you back at `@start`. |
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

## Editing tools (map viewer)

Step-by-step instructions for every editor: [HOW_TO.md](HOW_TO.md).

Open `tools/viewer/index.html` through the local viewer server
(http://localhost:8765/tools/viewer/index.html). Works offline.

| Tool | Status | Saves to | Apply with |
|---|---|---|---|
| Collision editor (delete generated pieces; native pieces too with "Include native collision") | ✅ / 🟡 native | `tools/edits/collision_edits.json` | `tools/rebuild_collision.sh` |
| Traffic editor (lanes and parked cars) | ✅ | `tools/edits/traffic_edits.json` | `tools/rebuild_traffic.sh` |
| Water editor (delete flooded squares) | ✅ | `tools/edits/water_edits.json` | `tools/rebuild_water.sh` |
| Spawn editor (no-spawn zones; shows F9 marks and removed spawns) | 🟡 | `tools/edits/spawn_blocks.json` | `tools/install_dlc.sh` (run `tools/sync_from_game.sh` first to see new marks / removals) |
| District aligner | ✅ | (values pasted to Claude) | — |
| Prop replacement table | ✅ | `tools/props_map.csv` (text file) | `tools/rebuild_props.sh` |
| Sidewalk editor (draw shapes to remove / add sidewalk for pedestrians) | 🟡 | `tools/edits/sidewalk_edits.json` | `tools/rebuild_navmesh.sh` |

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

Re-read every few seconds while playing. Running the setup again keeps your
values (new settings arrive with their defaults); the development install
script overwrites it with the project's copy.

| Setting | Now | Meaning |
|---|---|---|
| `@mode` | paradise | `off` = normal GTA |
| `@start` | 994.9 862.7 25.2 | Start / rescue spot |
| `@wreck` | on | Burnout wrecks |
| `@wreck_speed` | 18 | Sudden speed loss (m/s) that wrecks |
| `@impact_speed` | 11 | Sudden speed loss for extra dents |
| `@wreck_explode` | on | Car blows up in a wreck |
| `@respawn_speed` | 12 | Speed of the fresh car (m/s) |
| `@crash_time` | 5 | Slow-motion crash seconds |
| `@slowmo` | 0.15 | Game speed during a wreck |
| `@crash_cam` | on | Orbiting crash camera |
| `@precrash` | on | Start the slow-mo crash camera just before a wreck |
| `@precrash_lead` | 0.12 | How far ahead (game seconds) a wreck is seen coming |
| `@wreck_damage` | on | Wreck when on fire / dead / jammed |
| `@wreck_stuck` | 2.5 | Seconds jammed before a wreck |
| `@perf` | on | Log the plugin's cost per frame to `paradise.log` every 15 s |
| `@debug_log` | off | Old position probe and driving trace (debugging) |
| `@prop_keep` | 0.9 | Speed kept when hitting a prop |
| `@wreck_crush`, `@wreck_crush_rounds` | 8000, 8 | How mangled wrecked cars get |
| `@wreck_clear` | 6 | Radius (m) cleared of NPCs, vehicles and fires after a wreck (0 = off) |
| `@police_power`, `@police_top` | 2, 50 | Pursuing police engine power, extra top speed % |
| `@air_gravity` | 3 | Gravity on cars in the air |
| `@downforce` | 1 | Extra g on fast cars at ~180 km/h |
| `@boost` | on | Boost |
| `@boost_accel` | 9 | Boost acceleration (m/s²) |
| `@boost_top`, `@boost_min_top`, `@boost_max_top` | 1.8, 47, 95 | Boosted top speed (× car top, min/max m/s) |
| `@boost_seconds` | 7 | Full gauge duration |
| `@boost_fill` | 1.5 | Gauge refill rate |

---

## Ideas / not started

- Minimap and pause map of Paradise City (on hold)
- Takedowns and other Burnout mechanics
- Railings, sign gantries, chevrons as GTA props
- Full collision pass (known spots above)
- Pedestrian navmesh hand-editing beyond sidewalks (blocking / adding walkable areas)
- Water editor: add water / change lake levels
- LODs / distant city
