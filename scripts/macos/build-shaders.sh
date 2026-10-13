#!/bin/sh
# Compiles crates/choscorui-macos/src/shaders/ui.metal into ui.metallib
# and writes ui.metallib.stamp with the SHA-256 of both files. The
# `shader_library_matches_source` test of choscorui-macos reads the stamp.
#
# Needs the Metal toolchain. Since Xcode 26 it is a separate download:
#   xcodebuild -downloadComponent MetalToolchain
# The library targets macOS 15 and Metal 3.2, so one library serves Apple
# silicon and Intel.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
shaders=$root/crates/choscorui-macos/src/shaders
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

xcrun -sdk macosx metal -std=metal3.2 -mmacosx-version-min=15.0 -Os \
    -c "$shaders/ui.metal" -o "$work/ui.air"
xcrun -sdk macosx metallib "$work/ui.air" -o "$shaders/ui.metallib"

source_hash=$(shasum -a 256 "$shaders/ui.metal" | cut -d ' ' -f 1)
library_hash=$(shasum -a 256 "$shaders/ui.metallib" | cut -d ' ' -f 1)
printf 'ui.metal %s\nui.metallib %s\n' "$source_hash" "$library_hash" \
    > "$shaders/ui.metallib.stamp"
echo "wrote $shaders/ui.metallib"
