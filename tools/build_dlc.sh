#!/usr/bin/env bash
# Rebuilds the bpparadise DLC from Burnout collision and installs it into the
# GTA V *Legacy* OpenIV mods folder, plus the paradise.asi plugin and its
# teleport list. Never touches the Enhanced install.
#
#   tools/build_dlc.sh [collision-file] [placement args for bpx gta-collision...]
#   e.g. VIS_UNITS=all tools/build_dlc.sh ../WORLDCOL_EXTRA.BIN --offset 0,0,9.9
#   (9.9 puts Paradise's sea at GTA's z 0; see SEA_Z in crates/bpx/src/water.rs)
#   VIS_UNITS=322,323 (default 322) or VIS_UNITS=all picks the visual units.
#
# Requires: OpenIV.asi in the game folder for the mods folder to be used.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1

GTA="${GTA_DIR:-../Grand Theft Auto V}"
COL="${1:-../WORLDCOL_EXTRA.BIN}"
shift || true
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"

[ -f "$GTA/GTA5.exe" ] || { echo "GTA5.exe not found in $GTA"; exit 1; }
[ ! -f "$GTA/GTA5_Enhanced.exe" ] || { echo "refusing: $GTA is the Enhanced install"; exit 1; }

cargo build --release -q
dotnet build tools/cwconv -c Release -v q -nologo | grep -E " error " && exit 1 || true

rm -rf out/gta_col_xml out/gta_col out/gta_test_* out/gta_vis_xml out/gta_vis out/gta_meta_xml out/gta_meta out/dlc
./target/release/bpx gta-collision "$COL" out/gta_col_xml "$@"
# Streaming test slab above Paleto Bay (outside the Burnout footprint),
# shipped in the same archive as the map as a quick in-game sanity check.
./target/release/bpx gta-test-platform out/gta_col_xml bp_test_platform -150 6450 1015
# Visual models (same placement args as the collision).
./target/release/bpx gta-visual "${GAME_DIR:-../Burnout Paradise Remastered}" out/gta_vis_xml out/gta_meta_xml --units "${VIS_UNITS:-322}" "$@"
$CW convert out/gta_col_xml out/gta_col
$CW convert out/gta_vis_xml out/gta_vis
# Parked cars (Burnout's static traffic) as car generators in the metadata archive.
./target/release/bpx gta-cargens "${GAME_DIR:-../Burnout Paradise Remastered}" out/gta_meta_xml "$@"
# Burnout's props (lamp posts, traffic lights, bins...) as GTA props.
[ -f out/props/gta_archetypes.csv ] || { mkdir -p out/props; dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll archetypes "${GTA_DIR:-../Grand Theft Auto V}" "x64*.rpf" "levels/gta5/props/" out/props/gta_archetypes.csv; }
./target/release/bpx gta-props "${GAME_DIR:-../Burnout Paradise Remastered}" out/gta_meta_xml "$@" --col "${PROPS_COL:-../WORLDCOL_EXTRA.BIN}"
$CW convert out/gta_meta_xml out/gta_meta
$CW pack bpparadise out/dlc out/gta_col bpparadise_vis=out/gta_vis bpparadise_metadata=out/gta_meta:meta

# Sea around Paradise City (and where Los Santos was): GTA's water.xml plus
# generated quads, placed in the mods update.rpf overlay of common.rpf.
[ -f out/gta_water.xml ] || $CW extract "$GTA" 'common.rpf\data\levels\gta5\water.xml' out/gta_water.xml
./target/release/bpx water-meshes "${GAME_DIR:-../Burnout Paradise Remastered}" out/water_meshes.csv "$@"
./target/release/bpx gta-water "$COL" out/gta_water.xml out/water_paradise.xml --lakes out/water_meshes.csv "$@"

# Traffic paths: Burnout's traffic lanes as GTA path nodes (all 1024 areas, so
# Los Santos' roads are replaced), installed as the update's paths.rpf.
./target/release/bpx traffic-json "${GAME_DIR:-../Burnout Paradise Remastered}" out/traffic/traffic.json
./target/release/bpx gta-paths "${GAME_DIR:-../Burnout Paradise Remastered}" out/gta_paths_xml "$@"
rm -rf out/gta_paths
$CW convert out/gta_paths_xml out/gta_paths

# Pedestrian navmesh: Recast-built tiles from the collision, plus GTA's own
# vehicle navmeshes (trains, boats, trailers), installed as navmeshes0.rpf.
./target/release/bpx gta-navmesh "$COL" out/gta_navmesh_xml "$@"
rm -rf out/gta_navmesh
$CW convert out/gta_navmesh_xml out/gta_navmesh
[ -d out/vehicle_navmeshes ] || $CW rpf-extract "$GTA" x64f.rpf 'navmeshes[0-9]\.rpf/[^\[/]+\.ynv$' out/vehicle_navmeshes
cp out/vehicle_navmeshes/*.ynv out/gta_navmesh/

bash tools/install_dlc.sh
