#!/usr/bin/env bash
# Builds the folder (and zip) players download: the installer, the
# converters, the plugin and the mod's own data. No game files, and nothing
# from CodeWalker (players download it themselves).
#
#   tools/package_release.sh        -> dist/GTA-Paradise-<version>/ and .zip
#
# The version is paradise-setup's (crates/paradise-setup/Cargo.toml).
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/c/Program Files/dotnet:$PATH" DOTNET_NOLOGO=1

VERSION=$(grep -m1 '^version' crates/paradise-setup/Cargo.toml | cut -d'"' -f2)
NAME="GTA-Paradise-$VERSION"
DIST="dist/$NAME"

# Release builds without this PC's folder names inside them (Rust records
# source paths for error messages). RUSTFLAGS replaces .cargo/config.toml's
# flags, so the static C runtime is asked for here too.
HOME_WIN=$(cygpath -w "$HOME")
CARGO_HOME_WIN=$(cygpath -w "${CARGO_HOME:-$HOME/.cargo}")
PROJECT_WIN=$(cygpath -w "$PWD")
export RUSTFLAGS="-C target-feature=+crt-static --remap-path-prefix=$CARGO_HOME_WIN=cargo --remap-path-prefix=$PROJECT_WIN=gta-paradise --remap-path-prefix=$HOME_WIN=home"
cargo build --release -q --target-dir target/dist -p bpx -p paradise-asi -p paradise-setup

rm -rf "dist/cwconv"
dotnet publish tools/cwconv -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:EnableCompressionInSingleFile=true \
  -p:ForRelease=true -p:DebugType=none -o dist/cwconv -v q -nologo

rm -rf "$DIST" "$DIST.zip"
mkdir -p "$DIST/tools" "$DIST/mod" "$DIST/data/tools"
cp target/dist/release/paradise-setup.exe "$DIST/"
cp target/dist/release/bpx.exe "$DIST/tools/"
cp dist/cwconv/cwconv.exe "$DIST/tools/"
cp target/dist/release/paradise_asi.dll "$DIST/mod/paradise.asi"
cp -r tools/collision tools/edits "$DIST/data/tools/"
cp tools/props_map.csv tools/paradise.ini tools/base_map_disable.txt tools/base_map_packs.txt "$DIST/data/tools/"
cp docs/PLAYER_README.txt "$DIST/README.txt"
cp LICENSE "$DIST/LICENSE.txt"
echo "$VERSION" > "$DIST/version.txt"

# Nothing personal inside: no Windows user name in any file (as a whole
# word, so a GitHub name that contains it, like in LICENSE, is fine).
USER_NAME=$(basename "$HOME")
if grep -rlaiw -- "$USER_NAME" "$DIST" >/dev/null; then
  echo "refusing: these files contain '$USER_NAME':"; grep -rlaiw -- "$USER_NAME" "$DIST"; exit 1
fi

(cd dist && powershell -NoProfile -Command "Compress-Archive -Path '$NAME' -DestinationPath '$NAME.zip' -Force")
du -sh "$DIST" "dist/$NAME.zip"
echo "packaged: dist/$NAME.zip"
