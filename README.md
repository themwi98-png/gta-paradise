# GTA Paradise

## Video:
[![Watch the video](https://img.youtube.com/vi/2o_EHrRY4cg/maxresdefault.jpg)](https://www.youtube.com/watch?v=2o_EHrRY4cg)

Burnout Paradise City in GTA V story mode: the whole of Paradise City, from
**Burnout Paradise Remastered**, replaces Los Santos in **GTA V Legacy**. GTA's
traffic, pedestrians and police live in it, with Burnout-style driving on top.

- **True Open-World Paradise City:** custom collision meshes added for the vast majority of out-of-bounds surfaces. Still very rough, so expect collision glitches out of bounds, but there are far fewer areas to fall through the map. 
- **Burnout crashes:** cars crumple and wreck in slow motion with a crash
  camera, then you respawn nearby in a fresh copy of your car.
- **Boost:** a gauge you fill by driving fast, near misses, drifting, jumps
  and driving into oncoming traffic. Separate boost types not yet implemented.
- **Faster, grippier cars**, extra gravity for jumps, and downforce.
- **Street props** (lamp posts, traffic lights, benches, barriers…) placed
  where Burnout had them, and harmless to smash through.
- **Police** that can follow you around Paradise City.
- **A map viewer and editor** for hand-fixing collision, water, traffic lanes,
  sidewalks and spawn spots.

The full list, with what's confirmed working, is in
[docs/FEATURES.md](docs/FEATURES.md).

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
4. Start GTA V and load story mode.

## Settings

Most of the mod can be tuned in **`paradise.ini`**, in your GTA V folder (next
to `GTA5.exe`):

1. Open it with Notepad (right-click > Open with > Notepad).
2. Change a number, or `on` / `off`, after a setting's name. Lines starting
   with `#` are notes explaining each setting.
3. Save. **No restart needed:** the game picks up changes within a few
   seconds, so you can tweak while playing.

Updating the mod keeps your values. If you break something, delete the file
and run `paradise-setup.exe` again to get the defaults back.

The ones you're most likely to want:

| Setting | Default | What it does |
|---|---|---|
| `@wreck` | on | Hard crashes wreck the car (slow motion, then a fresh car) |
| `@wreck_speed` | 18 | How hard a hit has to be to wreck (sudden speed loss in m/s; 18 = 65 km/h) |
| `@slowmo` | 0.15 | Game speed during the wreck slow motion (1 = normal) |
| `@crash_time` | 5 | Seconds of slow motion before the respawn |
| `@precrash` | on | Start the slow motion just before a wreck you can't avoid |
| `@boost_seconds` | 7 | How long a full boost gauge lasts |
| `@boost_fill` | 1.5 | How fast the boost gauge refills (2 = twice as fast) |
| `@air_gravity` | 2 | How hard cars in the air are pulled down (1 = normal GTA) |
| `@downforce` | 1 | Grip from downforce at speed (0 = none, 2 = glued down) |
| `@police_power` | 2 | Engine power of police cars chasing you |
| `@mode` | paradise | `off` gives you normal GTA V back without uninstalling |

Every setting is described in the file itself and in
[docs/FEATURES.md](docs/FEATURES.md#settings-paradiseini-in-the-gta-folder).

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
