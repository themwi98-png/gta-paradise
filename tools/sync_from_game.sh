#!/usr/bin/env bash
# Copies what the game wrote for the viewer: F9 marks (paradise_marks.csv)
# and spawns the plugin removed (paradise_spawns.csv), into out/marks/ where
# the viewer's Spawns panel shows them.
set -euo pipefail
cd "$(dirname "$0")/.."
GTA="${GTA_DIR:-../Grand Theft Auto V}"
mkdir -p out/marks
for f in paradise_marks.csv paradise_spawns.csv; do
  if [ -f "$GTA/$f" ]; then cp "$GTA/$f" "out/marks/$f"; echo "copied $f"; fi
done
