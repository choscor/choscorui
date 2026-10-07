#!/bin/sh
# Installs the choscorui apps that sit next to this script.
#
#   ./install.sh                 installs into ~/Applications
#   CHOSCORUI_INSTALL_DIR=/Applications ./install.sh
#
# Remove everything again with ./uninstall.sh.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
target=${CHOSCORUI_INSTALL_DIR:-"$HOME/Applications"}

if [ "$(uname -s)" != Darwin ]; then
    echo "choscorui apps run on macOS only" >&2
    exit 1
fi
major=$(sw_vers -productVersion | cut -d. -f1)
if [ "$major" -lt 15 ]; then
    echo "choscorui needs macOS 15 or later (found $(sw_vers -productVersion))" >&2
    exit 1
fi

mkdir -p "$target"
found=0
for app in "$here"/*.app; do
    [ -d "$app" ] || continue
    found=1
    name=$(basename "$app")
    codesign --verify --strict "$app"
    rm -rf "${target:?}/$name"
    ditto "$app" "$target/$name"
    echo "installed $target/$name"
done
if [ "$found" = 0 ]; then
    echo "no .app bundles next to $0" >&2
    exit 1
fi
