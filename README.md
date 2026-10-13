# GTA Paradise

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

> **Work in progress.** There is no installer yet. The mod is currently built
> and installed with the scripts in `tools/` on the developer's own PC.

## No game files included

This repository contains only the mod's own code and hand-made edits. The map,
models, textures and other game data are converted on your PC from your own
copies of both games, and are never uploaded here.

## What you'll need

- **GTA V Legacy** (PC). The Enhanced edition and GTA Online are never touched;
  story mode only.
- **Burnout Paradise Remastered** (PC).
- [ScriptHookV](http://www.dev-c.com/gtav/scripthookv/) for the GTA build you
  have.
- [OpenIV](https://openiv.com/) with its `mods` folder set up (OpenIV.asi).

## Building it yourself (for now)

The tools are written in Rust and C# (.NET 8), driven by bash scripts (Git
Bash on Windows). Step-by-step instructions for the editors and the rebuild
scripts are in [docs/HOW_TO.md](docs/HOW_TO.md).

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
| `docs` | Feature list and how-to |

## Disclaimer

A fan project, not affiliated with or endorsed by Rockstar Games,
Take-Two Interactive, Criterion Games or Electronic Arts. Burnout Paradise and
Grand Theft Auto V belong to their respective owners. Use story mode only.
