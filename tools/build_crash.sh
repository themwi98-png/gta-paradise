#!/usr/bin/env bash
# Builds Burnout-strength crash damage (Rockstar's handling files with the
# damage multipliers scaled, for the mods update.rpf) and installs everything.
# Tune the multipliers in HANDLING, crates/bpx/src/gtadata.rs.
#
#   tools/build_crash.sh [--refresh]   (--refresh re-extracts Rockstar's files)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1
GTA="${GTA_DIR:-../Grand Theft Auto V}"
CW="dotnet tools/cwconv/bin/Release/net8.0-windows/cwconv.dll"
SRC=out/handling_src

# Rockstar's handling files, from the pristine (non-mods) archives.
if [ "${1:-}" = "--refresh" ] || [ ! -f "$SRC/_update/update/update.rpf/common/data/handling.meta" ]; then
  rm -rf "$SRC"
  $CW rpf-extract "$GTA" update/update.rpf 'handling\.meta$' "$SRC/_update" --keep-paths 2>/dev/null
  for p in $(ls "$GTA/update/x64/dlcpacks"); do
    $CW rpf-extract "$GTA" "update/x64/dlcpacks/$p/dlc.rpf" 'handling\.meta$' "$SRC/$p" 2>/dev/null | grep -v "extracted 0" || true
    [ -f "$SRC/$p/handling.meta" ] || { rm -rf "$SRC/$p"; continue; }
  done
fi

rm -rf out/crash out/dlc_crash
cargo build --release -q -p bpx
./target/release/bpx handling "$SRC" out/crash/update
bash tools/install_dlc.sh
