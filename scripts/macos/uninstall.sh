#!/bin/sh
# Removes the choscorui apps and the data they write for this user:
# settings (Application Support), preferences, caches, and saved window
# state.
#
#   ./uninstall.sh               removes apps from ~/Applications
#   ./uninstall.sh --keep-data   keeps settings and preferences
#   CHOSCORUI_INSTALL_DIR=/Applications ./uninstall.sh
set -eu

target=${CHOSCORUI_INSTALL_DIR:-"$HOME/Applications"}
keep_data=0
[ "${1:-}" = --keep-data ] && keep_data=1

# app name | bundle identifier
apps="choscorui Gallery|com.choscor.gallery
choscorui Stress|com.choscor.stress
choscorui Notes|com.choscor.notes"

echo "$apps" | while IFS='|' read -r name bundle_id; do
    if [ -d "$target/$name.app" ]; then
        rm -rf "${target:?}/$name.app"
        echo "removed $target/$name.app"
    fi
    [ "$keep_data" = 1 ] && continue
    for path in \
        "$HOME/Library/Application Support/$bundle_id" \
        "$HOME/Library/Caches/$bundle_id" \
        "$HOME/Library/HTTPStorages/$bundle_id" \
        "$HOME/Library/Saved Application State/$bundle_id.savedState" \
        "$HOME/Library/Preferences/$bundle_id.plist"; do
        if [ -e "$path" ]; then
            rm -rf "$path"
            echo "removed $path"
        fi
    done
    # Clears the preferences daemon's cached copy as well as the file.
    defaults delete "$bundle_id" >/dev/null 2>&1 || true
done
