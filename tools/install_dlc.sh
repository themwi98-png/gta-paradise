#!/usr/bin/env bash
# Installs an already-built bpparadise DLC (out/dlc, out/water_paradise.xml,
# plugin and overlay) into the GTA V *Legacy* OpenIV mods folder.
# tools/build_dlc.sh builds and then runs this; run it alone to retry an
# install (e.g. after closing the game, which locks the installed files).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1

GTA="${GTA_DIR:-../Grand Theft Auto V}"
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"

[ -f "$GTA/GTA5.exe" ] || { echo "GTA5.exe not found in $GTA"; exit 1; }
[ ! -f "$GTA/GTA5_Enhanced.exe" ] || { echo "refusing: $GTA is the Enhanced install"; exit 1; }
if tasklist //FI "IMAGENAME eq GTA5.exe" 2>/dev/null | grep -qi gta5.exe; then
  echo "GTA V is running and locks the installed files; close it and run tools/install_dlc.sh again."
  exit 2
fi

mkdir -p "$GTA/mods/update/x64/dlcpacks/bpparadise"
cp out/dlc/bpparadise/dlc.rpf "$GTA/mods/update/x64/dlcpacks/bpparadise/dlc.rpf"
if [ ! -f "$GTA/mods/update/update.rpf" ]; then
  cp "$GTA/update/update.rpf" "$GTA/mods/update/update.rpf"
fi
$CW dlclist-add "$GTA" bpparadise "$GTA/mods/update/update.rpf" --open
# The old bpcrash pack (handling override DLC) crashed the game on loading story mode.
$CW dlclist-remove "$GTA" bpcrash "$GTA/mods/update/update.rpf" --open
rm -rf "$GTA/mods/update/x64/dlcpacks/bpcrash"
# Remove Los Santos: drop its archives from the level's data file lists
# (DLC change sets cannot unload base-map archives). Always starts from the
# pristine update.rpf copies; tools/base_map_disable.txt lists the archives.
mkdir -p out/base_meta
$CW extract "$GTA" 'update\update.rpf\common\data\levels\gta5\images.meta' out/base_meta/images.orig.meta
$CW extract "$GTA" 'update\update.rpf\common\data\levels\gta5\gta5.meta' out/base_meta/gta5.orig.meta
python -I tools/strip_base_map.py tools/base_map_disable.txt out/base_meta/images.orig.meta out/base_meta/gta5.orig.meta out/base_meta
$CW rpf-put "$GTA" "$GTA/mods/update/update.rpf" common/data/levels/gta5/images.meta out/base_meta/images.meta --open
$CW rpf-put "$GTA" "$GTA/mods/update/update.rpf" common/data/levels/gta5/gta5.meta out/base_meta/gta5.meta --open
# Rockstar's patch packs re-add updated Los Santos sectors; give the mods
# folder a copy of each with those entries stripped from its content.xml.
grep -v '^#' tools/base_map_packs.txt | while read -r p; do
  [ -n "$p" ] || continue
  src="$GTA/update/x64/dlcpacks/$p/dlc.rpf"
  dst="$GTA/mods/update/x64/dlcpacks/$p/dlc.rpf"
  mkdir -p "$(dirname "$dst")"
  [ -f "$dst" ] || cp "$src" "$dst"
  $CW extract "$GTA" "update\\x64\\dlcpacks\\$p\\dlc.rpf\\content.xml" "out/base_meta/$p.orig.xml" > /dev/null
  python -I tools/strip_base_map.py pack "out/base_meta/$p.orig.xml" "out/base_meta/$p.xml"
  $CW rpf-put "$GTA" "$dst" content.xml "out/base_meta/$p.xml" --open
  # The game still mounts some of these archives with no data file naming
  # them (seen for patchday1ng/3ng), so empty them as well.
  $CW rpf-empty "$GTA" "$dst" 'levels/gta5/(_citye|_cityw|_hills)/|levels/gta5/[^/]*_additions[^/]*\.rpf$' --open
done
if [ -d out/gta_paths ]; then
  $CW rpf-replace "$GTA" "$GTA/mods/update/update.rpf" x64/levels/gta5/paths.rpf out/gta_paths --open
fi
if [ -d out/gta_navmesh ]; then
  # Our navmesh replaces Los Santos': navmeshes0.rpf holds it, 1-9 are empty.
  $CW rpf-replace "$GTA" "$GTA/mods/update/update.rpf" x64/levels/gta5/navmeshes0.rpf out/gta_navmesh --open
  for i in 1 2 3 4 5 6 7 8 9; do
    $CW rpf-replace "$GTA" "$GTA/mods/update/update.rpf" x64/levels/gta5/navmeshes$i.rpf out/empty_dir_unused --open
  done
fi
# Population zones from Burnout's districts (LSPD everywhere, peds on the island).
if [ -f out/debug/districts.npy ]; then
  $CW extract "$GTA" 'update\update.rpf\common\data\levels\gta5\popzone.ipl' out/base_meta/popzone.orig.ipl > /dev/null
  python -I tools/make_popzone.py out/debug/districts.npy out/base_meta/popzone.orig.ipl out/popzone_paradise.ipl
  $CW rpf-put "$GTA" "$GTA/mods/update/update.rpf" common/data/levels/gta5/popzone.ipl out/popzone_paradise.ipl --open
fi
# Burnout-strength crash damage (tools/build_crash.sh): Rockstar's handling
# files with the damage multipliers scaled, at their update.rpf paths.
if [ -d out/crash/update ]; then
  (cd out/crash/update && find . -type f) | while read -r f; do
    $CW rpf-put "$GTA" "$GTA/mods/update/update.rpf" "${f#./}" "out/crash/update/${f#./}" --open
  done
fi
if [ -f out/water_paradise.xml ]; then
  $CW rpf-put "$GTA" "$GTA/mods/update/update.rpf" common/data/levels/gta5/water.xml out/water_paradise.xml --open
fi
cp target/release/paradise_asi.dll "$GTA/paradise.asi"
cp tools/paradise.ini "$GTA/paradise.ini"
cp out/gta_col_xml/paradise_col.bin "$GTA/paradise_col.bin"
[ -f out/props/paradise_props.csv ] && cp out/props/paradise_props.csv "$GTA/paradise_props.csv"
# No-spawn zones placed in the viewer's Spawns panel.
[ -f tools/edits/spawn_blocks.json ] && cp tools/edits/spawn_blocks.json "$GTA/paradise_spawn_blocks.json"
echo "installed: $GTA/mods/update/x64/dlcpacks/bpparadise/dlc.rpf, water.xml, paradise.asi, paradise.ini, paradise_col.bin"
