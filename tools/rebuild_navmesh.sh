#!/usr/bin/env bash
# Quick rebuild after sidewalk edits (tools/edits/sidewalk_edits.json, from
# the viewer's Sidewalks panel): pedestrian navmesh only, then installed.
# Close GTA first.
#
#   tools/rebuild_navmesh.sh [collision-file] [placement args...]
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"

COL="${1:-../WORLDCOL_EXTRA.BIN}"
shift || true
[ $# -gt 0 ] || set -- --offset 0,0,9.9

cargo build --release -q
./target/release/bpx gta-navmesh "$COL" out/gta_navmesh_xml "$@"
rm -rf out/gta_navmesh
$CW convert out/gta_navmesh_xml out/gta_navmesh
cp out/vehicle_navmeshes/*.ynv out/gta_navmesh/
bash tools/install_dlc.sh
