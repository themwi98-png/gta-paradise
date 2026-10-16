# GTA Paradise

[Download](../../releases) (detailed installation instructions below)

## Video of new drifting and more:
[![Watch the video](https://img.youtube.com/vi/uDZeJiHGrR0/maxresdefault.jpg)](https://www.youtube.com/watch?v=uDZeJiHGrR0)

Burnout Paradise City in GTA V story mode: the whole of Paradise City, from
**Burnout Paradise Remastered**, replaces Los Santos in **GTA V Legacy**. GTA's
traffic, pedestrians and police live in it, with Burnout-style driving on top.

- **True Open-World Paradise City:** custom collision meshes added for the vast majority of out-of-bounds surfaces. Still very rough, so expect collision glitches out of bounds, but there are far fewer areas to fall through the map. 
- **Burnout crashes:** cars crumple and wreck in slow motion with a crash
  camera, then you respawn nearby in a fresh copy of your car. Smashed-up
  looks don't stop you, but a wrecked engine does.
- **Burnout driving:** drift assist, brake-to-drift, steady flight off jumps
  and barrel rolls off split ramps. Off-road is as fast as the road.
- **Boost:** a gauge you fill by driving fast, near misses, drifting, jumps
  and driving into oncoming traffic. Separate boost types not yet implemented.
- **Retuned cars:** faster, harder braking, extra gravity for jumps, and
  downforce.
- **Street props** (lamp posts, traffic lights, benches, barriers…) placed
  where Burnout had them. Smashing through them is purely visual: they never
  slow you down or knock you off line.
- **Traffic** that's busy downtown and quiet on country roads, following
  Burnout's own traffic data.
- **Police** that can follow you around Paradise City.
- **A map viewer and editor** for hand-fixing collision, water, traffic lanes,
  sidewalks and spawn spots.

The full list, with what's confirmed working, is in
[docs/FEATURES.md](docs/FEATURES.md); what changed in each release is in
[CHANGELOG.md](CHANGELOG.md).

## Known issues

- **Some tunnels are blocked or flooded.** Glitched collision blocks the way
  through some tunnels, and one tunnel is flooded with water.
- **The minimap and GPS still show Los Santos.** The pause-menu map, minimap
  and route guidance haven't been replaced yet.
- **Burnout's low-detail backdrop hills are missing**, so the distant
  scenery around the edge of the city isn't there.
- **Low-detail (LOD) models are missing.** Buildings and scenery only have
  their full-detail models, so far-away parts of the city don't show until
  you get closer.
- **Police sometimes bug out while following you**, likely because of the
  mod's higher car speeds and weaker cars, which GTA's police driving
  wasn't made for.
- **Out-of-bounds collision is rough.** Expect some glitches off the main
  roads (see the first feature above).
- **Various visual glitches** Power lines are white for some reason, some road textures don't render properly

## No game files included

This repository contains only the mod's own code and hand-made edits. The map,
models, textures and other game data are converted on your PC from your own
copies of both games, and are never uploaded here.

## What you'll need

- **GTA V Legacy** (PC). The Enhanced edition is not touched;
  story mode only.
- **Burnout Paradise Remastered** (PC).
- [ScriptHookV](http://www.dev-c.com/gtav/scripthookv/) for the GTA build you
  have.
- [OpenIV](https://openiv.com/) with its `mods` folder set up (OpenIV.asi).
- [CodeWalker](https://www.gta5-mods.com/tools/codewalker-gtav-interactive-3d-map) 3.0 (dev46 or newer), used to build the GTA files. Not included: its author hasn't released it under a licence that allows bundling it.

## Installing

1. Install the above, and close GTA V.
2. Download the latest `GTA-Paradise-<version>.zip` from
   [Releases](../../releases), unzip it anywhere and run `paradise-setup.exe`.
   (Windows may warn that the app isn't signed: click **More info**, then
   **Run anyway**.)
3. The setup finds your games and CodeWalker, or lets you pick their folders.
   Press **Install**: the first install converts the whole city (about
   10–20 minutes, 10 GB free disk space). Updates only redo what changed.
4. Add -nobattleye to GTA V's launch options in steam.
5. Start GTA V and load story mode.

## Settings

Most of the mod can be tuned in **`paradise.ini`**, in your GTA V folder (next
to `GTA5.exe`):

1. Open it with Notepad (right-click > Open with > Notepad).
2. Change a number, or `on` / `off`, after a setting's name. Lines starting
   with `#` are notes explaining each setting.
3. Save. **No restart needed:** the game picks up changes within a few
   seconds, so you can tweak while playing. The exceptions are `@car_grip`
   and `@car_brakes`, which are built into the car data: run
   `paradise-setup.exe` again after changing them.

Updating the mod keeps the values you've changed; settings you never touched
move to the new defaults. If you break something, delete the file and run
`paradise-setup.exe` again to get the defaults back.

The ones you're most likely to want:

| Setting | Default | What it does |
|---|---|---|
| `@wreck` | on | Hard crashes wreck the car (slow motion, then a fresh car) |
| `@wreck_speed` | 25 | How hard a hit has to be to wreck (sudden speed loss in m/s; 25 = 90 km/h) |
| `@crash_cam_speed` | 20 | Hits at least this hard (m/s; 20 = 72 km/h) show the slow-motion crash camera |
| `@wreck_engine` | 100 | Engine health below which the car is wrecked (1000 = new) |
| `@slowmo` | 0.15 | Game speed during the wreck slow motion (1 = normal) |
| `@crash_time` | 5 | Seconds of slow motion before the respawn |
| `@precrash` | on | Start the slow motion just before a wreck you can't avoid |
| `@wreck_eject` | on | In a wreck you can fly through the windscreen (never in a normal crash) |
| `@boost_seconds` | 15 | How long a full boost gauge lasts |
| `@boost_fill` | 1.5 | How fast the boost gauge refills (2 = twice as fast) |
| `@air_gravity` | 3 | How hard cars in the air are pulled down (1 = normal GTA) |
| `@air_control` | 0.7 | How steady cars stay in the air (0 = off, 1 = held steady) |
| `@barrel_roll_trigger` | 90 | How hard a split ramp must roll you for a barrel roll (higher = fewer) |
| `@downforce` | 0.75 | Grip from downforce at speed (0 = none, 2 = glued down) |
| `@drift_assist` | 1.1 | Drift help: holds slides steady (0 = off, up to 3) |
| `@brake_drift` | on | Tapping the brake while turning starts a drift, Burnout style |
| `@brake_drift_steer` | 0.3 | How far you must be steering for that (0 = going straight too) |
| `@car_grip` | 1.2 | Grip, times GTA's (re-run the setup after changing) |
| `@car_brakes` | 2.5 | Braking power, times GTA's (re-run the setup after changing) |
| `@traffic` | 1 | Amount of traffic (0 = empty roads, up to 3) |
| `@police_power` | 1 | Engine power of police cars chasing you, times GTA's |
| `@start_car` | drifttampa | The car you start in (any GTA vehicle model; `none` = on foot) |
| `@mode` | paradise | `off` gives you normal GTA V back without uninstalling |

Every setting is described in the file itself and in
[docs/FEATURES.md](docs/FEATURES.md#settings-paradiseini-in-the-gta-folder).

## Original Video
[![Watch the video](https://img.youtube.com/vi/2o_EHrRY4cg/maxresdefault.jpg)](https://www.youtube.com/watch?v=2o_EHrRY4cg)

## Project layout

| Folder | What it is |
|---|---|
| `crates/bpx` | Command-line converter: Burnout data → GTA map, collision, roads, navmesh, water, props |
| `crates/paradise-asi` | The in-game plugin (ScriptHookV): crashes, boost, police, prop handling, spawn clean-up |
| `crates/bpworld`, `crates/bnd2` | Readers for Burnout Paradise's file formats |
| `tools/cwconv` | Small C# tool built on CodeWalker for GTA file conversion and packing |
| `tools/viewer` | Browser-based map viewer and editors |
| `tools/edits` | Hand edits made in the viewer (collision, water, traffic, spawn zones) |
| `tools/*.sh` | Build, rebuild and install scripts |
| `third_party/rerecast` | Navmesh library (MIT / Apache-2.0), lightly patched |
| `crates/paradise-setup` | The installer (setup window and console version) |
| `docs` | Feature list, how-to for the editors and scripts |

## License

The mod's own code and data are under the [MIT License](LICENSE). That covers
this project only: it gives no rights to Burnout Paradise, GTA V or any of
their content. `third_party/rerecast` keeps its own licence (MIT / Apache-2.0).

## Disclaimer

A fan project, not affiliated with or endorsed by Rockstar Games,
Take-Two Interactive, Criterion Games or Electronic Arts. Burnout Paradise and
Grand Theft Auto V belong to their respective owners. Use story mode only.
