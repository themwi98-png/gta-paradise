#!/usr/bin/env bash
# Quick rebuild after traffic edits (tools/edits/traffic_edits.json, from the
# viewer's traffic editor): GTA roads (paths) and parked cars, repacked with
# the existing collision and models, then installed.
#
#   tools/rebuild_traffic.sh [placement args...]   (default --offset 0,0,9.9)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1
[ $# -gt 0 ] || set -- --offset 0,0,9.9
GAME="${GAME_DIR:-../Burnout Paradise Remastered}"
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"
[ -d out/gta_col ] && [ -d out/gta_vis ] && [ -d out/gta_meta_xml ] || { echo "no built map in out/; run tools/build_dlc.sh once first"; exit 1; }

cargo build --release -q
./target/release/bpx traffic-json "$GAME" out/traffic/traffic.json

rm -rf out/gta_paths_xml out/gta_paths
./target/release/bpx gta-paths "$GAME" out/gta_paths_xml "$@"
$CW convert out/gta_paths_xml out/gta_paths

./target/release/bpx gta-cargens "$GAME" out/gta_meta_xml "$@"
# Burnout's props (lamp posts, traffic lights, bins...) as GTA props.
[ -f out/props/gta_archetypes.csv ] || { mkdir -p out/props; dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll archetypes "${GTA_DIR:-../Grand Theft Auto V}" "x64*.rpf" "levels/gta5/props/" out/props/gta_archetypes.csv; }
./target/release/bpx gta-props "${GAME_DIR:-../Burnout Paradise Remastered}" out/gta_meta_xml "$@" --col "${PROPS_COL:-tools/collision/paradise_extra.pcx}"
rm -rf out/gta_meta out/dlc
$CW convert out/gta_meta_xml out/gta_meta
$CW pack bpparadise out/dlc out/gta_col bpparadise_vis=out/gta_vis bpparadise_metadata=out/gta_meta:meta

bash tools/install_dlc.sh
