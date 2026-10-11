#!/bin/sh
# Release gate: installs a packaged archive the way a user would, runs
# every core scenario from the installed bundles, uninstalls, and checks
# that nothing is left behind.
#
#   scripts/macos/install-check.sh dist/choscorui-<version>-macos-<arch>.zip
#
# Needs a logged-in GUI session (the apps open real windows). It refuses to
# run if the user already has data for the demo apps, so it can never
# delete anything it did not create.
set -eu

archive=${1:?usage: install-check.sh <archive.zip>}
archive=$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")
work=$(mktemp -d "${TMPDIR:-/tmp}/choscorui-install-check.XXXXXX")
trap 'rm -rf "$work"' EXIT
ids="com.choscor.gallery com.choscor.stress com.choscor.notes"
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

leftovers() { # prints any per-user data for the demo apps
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
    echo "refusing to run: data for the demo apps already exists:" >&2
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

check "install.sh installs three signed apps" "$package/install.sh"
check "three apps installed" test "$(find "$apps" -maxdepth 1 -name '*.app' | wc -l | tr -d ' ')" = 3
for app in "$apps"/*.app; do
    check "signature valid: $(basename "$app")" codesign --verify --deep --strict "$app"
done

gallery="$apps/choscorui Gallery.app"
stress="$apps/choscorui Stress.app"
notes="$apps/choscorui Notes.app"
check "gallery launches through LaunchServices and draws" \
    open -n -W --env CHOSCORUI_GALLERY_AUTOCLOSE_MS=1500 "$gallery"
check "gallery first frame" \
    env CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 "$gallery/Contents/MacOS/choscorui-gallery"
check "stress autopilot (real per-user settings)" \
    env CHOSCORUI_STRESS_AUTOPILOT=1 "$stress/Contents/MacOS/choscorui-stress"
check "stress interactive launch saves settings on exit" \
    env CHOSCORUI_STRESS_AUTOCLOSE_MS=1500 "$stress/Contents/MacOS/choscorui-stress"
check "stress wrote its settings file" \
    test -f "$HOME/Library/Application Support/com.choscor.stress/settings.txt"
check "notes autopilot with bundled translations" \
    env NOTES_AUTOPILOT=1 "$notes/Contents/MacOS/choscorui-notes-sample"
check "notes found translations inside the bundle" \
    test -f "$notes/Contents/Resources/strings/he.lang"

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
