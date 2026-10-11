#!/bin/sh
# Reads a running app's accessibility tree from outside the process,
# through the same macOS accessibility API that VoiceOver uses, and
# checks that the expected controls are exposed with the right roles and
# names.
#
#   scripts/macos/ax-check.sh            # builds and checks the gallery in English and Vietnamese
#
# Needs the terminal (or CI agent) to be allowed under System Settings →
# Privacy & Security → Accessibility. Without that permission it exits 2.
#
# This is an optional diagnostic. It walks `UI elements` recursively,
# because System Events cannot resolve the items of `entire contents` for
# the toolkit's accessibility elements.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/choscorui-ax.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0

cat >"$work/dump.applescript" <<'EOF'
on walk(e, depth)
  tell application "System Events"
    set r to ""
    set d to ""
    set t to ""
    try
      set r to role of e
    end try
    try
      set d to description of e
    end try
    try
      set t to title of e
    end try
    set out to r & "|" & d & "|" & t & linefeed
    if depth < 12 then
      try
        repeat with c in (UI elements of e)
          set out to out & my walk(c, depth + 1)
        end repeat
      end try
    end if
  end tell
  return out
end walk

on run argv
  set pid to (item 1 of argv) as integer
  set out to ""
  tell application "System Events"
    set p to first process whose unix id is pid
    repeat with w in windows of p
      set out to out & "AXWindow|" & (name of w) & linefeed
      repeat with c in (UI elements of w)
        set out to out & my walk(c, 1)
      end repeat
    end repeat
  end tell
  return out
end run
EOF

# Reading another process's UI elements fails ("not allowed assistive access") until the
# terminal has accessibility access.
if ! osascript -e 'tell application "System Events" to get windows of (first process whose frontmost is true)' >/dev/null 2>&1; then
    echo "accessibility access is not granted to this terminal" >&2
    echo "allow it in System Settings → Privacy & Security → Accessibility" >&2
    exit 2
fi

dump() { # binary page-id env... -> writes $work/tree.txt
    binary=$1
    page=$2
    shift 2
    env "$@" "$binary" -- "$page" >/dev/null 2>&1 &
    pid=$!
    tree=""
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        sleep 1
        tree=$(osascript "$work/dump.applescript" "$pid" 2>/dev/null || true)
        [ -n "$tree" ] && break
    done
    printf '%s\n' "$tree" >"$work/tree.txt"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

expect() { # description pattern (extended regex over role|description|title)
    if grep -Eq "$2" "$work/tree.txt"; then
        echo "ok   $1"
    else
        echo "FAIL $1 (no line matching /$2/)"
        failures=$((failures + 1))
    fi
}

cargo build --quiet --release -p choscorui-gallery --manifest-path "$root/Cargo.toml"
# Private settings, so the check neither reads nor changes the user's.
settings="$work/settings.txt"
dump "$root/target/release/choscorui-gallery" slider CHOSCORUI_GALLERY_SETTINGS="$settings" \
    CHOSCORUI_GALLERY_AUTOCLOSE_MS=120000 CHOSCORUI_GALLERY_LANG=en
expect "gallery: window title names the page" '^AXWindow\|choscorui gallery — Slider'
expect "gallery: theme button" '^AXButton\|.*Dark'
expect "gallery: Enabled toggle" '^AXCheckBox\|.*Enabled'
expect "gallery: sidebar filter" '^AXTextField\|.*Filter pages'
expect "gallery: new window button" '^AXButton\|.*Open in new window'
expect "gallery: slider page control" '^AXSlider\|'

dump "$root/target/release/choscorui-gallery" slider CHOSCORUI_GALLERY_SETTINGS="$settings" \
    CHOSCORUI_GALLERY_AUTOCLOSE_MS=120000 CHOSCORUI_GALLERY_LANG=vi
expect "gallery (vi): Vietnamese window title" '^AXWindow\|choscorui gallery — Thanh trượt'
expect "gallery (vi): Vietnamese theme button" '^AXButton\|.*Tối'
expect "gallery (vi): Vietnamese Enabled toggle" '^AXCheckBox\|.*Bật'
expect "gallery (vi): Vietnamese sidebar filter" '^AXTextField\|.*Lọc trang'

if [ "$failures" = 0 ]; then
    echo "OK (0 failures)"
else
    echo "--- last tree ---"
    cat "$work/tree.txt"
    echo "FAILED ($failures failures)"
    exit 1
fi
