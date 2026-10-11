#!/bin/sh
# Release gate: installs a packaged archive the way a user would, runs
# every core scenario from the installed bundle, uninstalls, and checks
# that nothing is left behind.
#
#   scripts/macos/install-check.sh dist/choscorui-<version>-macos-<arch>.zip
#
# Needs a logged-in GUI session (the gallery opens real windows). It
# refuses to run if the user already has data for the gallery, so it can
# never delete anything it did not create.
set -eu

archive=${1:?usage: install-check.sh <archive.zip>}
archive=$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")
work=$(mktemp -d "${TMPDIR:-/tmp}/choscorui-install-check.XXXXXX")
trap 'rm -rf "$work"' EXIT
ids="com.choscor.gallery"
failures=0

check() { # description command...
    description=$1
    shift
    if "$@" >"$work/last.log" 2>&1; then
        echo "ok   $description"
    else
        echo "FAIL $description"
        sed 's/^/     /' "$work/last.log" | tail -n 15
        failures=$((failures + 1))
    fi
}

leftovers() { # prints any per-user data for the gallery
    for id in $ids; do
        for path in \
            "$HOME/Library/Application Support/$id" \
            "$HOME/Library/Caches/$id" \
            "$HOME/Library/HTTPStorages/$id" \
            "$HOME/Library/Saved Application State/$id.savedState" \
            "$HOME/Library/Preferences/$id.plist"; do
            [ -e "$path" ] && echo "$path"
        done
        defaults read "$id" >/dev/null 2>&1 && echo "defaults domain $id"
    done
    return 0
}

existing=$(leftovers)
if [ -n "$existing" ]; then
    echo "refusing to run: data for the gallery already exists:" >&2
    echo "$existing" >&2
    echo "remove it with uninstall.sh first" >&2
    exit 2
fi

# A downloaded archive: unpack it into an empty directory.
ditto -x -k "$archive" "$work/download"
package=$(find "$work/download" -mindepth 1 -maxdepth 1 -type d | head -n 1)
apps="$work/Applications"
for file in README.md LICENSE-MIT LICENSE-APACHE THIRD-PARTY-NOTICES.txt; do
    check "archive contains $file" test -f "$package/$file"
done
export CHOSCORUI_INSTALL_DIR="$apps"

check "install.sh installs one signed app" "$package/install.sh"
check "one app installed" test "$(find "$apps" -maxdepth 1 -name '*.app' | wc -l | tr -d ' ')" = 1
for app in "$apps"/*.app; do
    check "signature valid: $(basename "$app")" codesign --verify --deep --strict "$app"
done

gallery="$apps/choscorui Gallery.app"
check "gallery launches through LaunchServices and draws" \
    open -n -W --env CHOSCORUI_GALLERY_AUTOCLOSE_MS=1500 "$gallery"
check "gallery interactive launch saved its settings on exit" \
    test -f "$HOME/Library/Application Support/com.choscor.gallery/settings.txt"
check "gallery first frame" \
    env CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 "$gallery/Contents/MacOS/choscorui-gallery"
check "gallery autopilot (real per-user settings, bundled translations)" \
    env CHOSCORUI_GALLERY_AUTOPILOT=1 "$gallery/Contents/MacOS/choscorui-gallery"
check "gallery translations are inside the bundle" \
    test -f "$gallery/Contents/Resources/strings/vi.lang"

check "uninstall.sh runs" "$package/uninstall.sh"
check "no apps left in the install directory" \
    test -z "$(find "$apps" -mindepth 1 -maxdepth 1)"
remaining=$(leftovers)
check "no per-user data left" test -z "$remaining"
[ -n "$remaining" ] && echo "$remaining" | sed 's/^/     /'

if [ "$failures" = 0 ]; then
    echo "OK (0 failures)"
else
    echo "FAILED ($failures failures)"
    exit 1
fi
