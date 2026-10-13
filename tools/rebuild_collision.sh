#!/usr/bin/env bash
# Quick rebuild after collision edits (tools/edits/collision_edits.json, from
# the viewer's collision editor): GTA collision, water and pedestrian navmesh,
# repacked with the existing visual models, then installed. The 3D models are
# not rebuilt (tools/build_dlc.sh does everything).
#
#   tools/rebuild_collision.sh [collision-file] [placement args...]
#   e.g. tools/rebuild_collision.sh ../WORLDCOL_EXTRA.BIN --offset 0,0,9.9
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1

COL="${1:-../WORLDCOL_EXTRA.BIN}"
shift || true
[ $# -gt 0 ] || set -- --offset 0,0,9.9
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"
[ -d out/gta_vis ] && [ -d out/gta_meta ] || { echo "no built models in out/; run tools/build_dlc.sh once first"; exit 1; }

cargo build --release -q

rm -rf out/gta_col_xml out/gta_col out/dlc
./target/release/bpx gta-collision "$COL" out/gta_col_xml "$@"
./target/release/bpx gta-test-platform out/gta_col_xml bp_test_platform -150 6450 1015
$CW convert out/gta_col_xml out/gta_col
$CW pack bpparadise out/dlc out/gta_col bpparadise_vis=out/gta_vis bpparadise_metadata=out/gta_meta:meta

./target/release/bpx gta-water "$COL" out/gta_water.xml out/water_paradise.xml --lakes out/water_meshes.csv "$@"

./target/release/bpx gta-navmesh "$COL" out/gta_navmesh_xml "$@"
rm -rf out/gta_navmesh
$CW convert out/gta_navmesh_xml out/gta_navmesh
cp out/vehicle_navmeshes/*.ynv out/gta_navmesh/

bash tools/install_dlc.sh
