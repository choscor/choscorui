#!/bin/sh
# Reads a running app's accessibility tree from outside the process,
# through the same macOS accessibility API that VoiceOver uses, and
# checks that the expected controls are exposed with the right roles and
# names.
#
#   scripts/macos/ax-check.sh            # builds and checks the gallery and notes
#
# Needs the terminal (or CI agent) to be allowed under System Settings →
# Privacy & Security → Accessibility. Without that permission it exits 2.
#
# Status: not yet run against a real session (access was never granted on
# the development machine). The expected roles and names come from the
# accessibility bridge's mapping in docs/ACCESSIBILITY.md and may need
# adjusting on the first run.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/choscorui-ax.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0

cat >"$work/dump.applescript" <<'EOF'
on run argv
  set pid to (item 1 of argv) as integer
  set out to ""
  tell application "System Events"
    set p to first process whose unix id is pid
    repeat with w in windows of p
      set out to out & "AXWindow|" & (name of w) & linefeed
      repeat with e in (entire contents of w)
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
        set out to out & r & "|" & d & "|" & t & linefeed
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

dump() { # binary env... -> writes $work/tree.txt
    binary=$1
    shift
    env "$@" "$binary" >/dev/null 2>&1 &
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
dump "$root/target/release/choscorui-gallery" CHOSCORUI_GALLERY_AUTOCLOSE_MS=15000
expect "gallery: theme button" '^AXButton\|.*Switch theme'
expect "gallery: checkbox" '^AXCheckBox\|.*Show selection'
expect "gallery: text field" '^AXTextField\|.*Greeting'
expect "gallery: multiline document" '^AXTextArea\|.*Multilingual document'
expect "gallery: scrollable rows" 'Row 1'

(cd "$root/samples/notes" && cargo build --quiet --release)
dump "$root/samples/notes/target/release/choscorui-notes-sample" NOTES_LANG=he
expect "notes: Hebrew window title" '^AXWindow\|פתק 1'
expect "notes: Hebrew toolbar button" '^AXButton\|.*חלון חדש'
expect "notes: Hebrew text field" '^AXTextField\|.*טקסט הפתק'
expect "notes: status text" '^AXStaticText\|.*חלון אחד פתוח'

if [ "$failures" = 0 ]; then
    echo "OK (0 failures)"
else
    echo "--- last tree ---"
    cat "$work/tree.txt"
    echo "FAILED ($failures failures)"
    exit 1
fi
