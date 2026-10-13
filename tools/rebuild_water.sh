#!/usr/bin/env bash
# Quick rebuild after water edits (tools/edits/water_edits.json, from the
# viewer's water editor): regenerates water.xml and the viewer's water data,
# then installs. Close GTA first.
#
#   tools/rebuild_water.sh [collision-file] [placement args...]
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

COL="${1:-tools/collision/paradise_extra.pcx}"
shift || true
[ $# -gt 0 ] || set -- --offset 0,0,9.9

cargo build --release -q
./target/release/bpx gta-water "$COL" out/gta_water.xml out/water_paradise.xml --lakes out/water_meshes.csv "$@"
bash tools/install_dlc.sh
