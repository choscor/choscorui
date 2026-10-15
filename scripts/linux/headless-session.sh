#!/usr/bin/env bash
# Runs a command in a headless desktop session:
#
#   scripts/linux/headless-session.sh COMMAND [ARGS...]
#
# The session has a D-Bus session bus, the AT-SPI registry, a headless
# sway compositor with one 1280x800 output, and Xvfb on :99 with RandR.
# The command gets WAYLAND_DISPLAY and DISPLAY. CHOSCORUI_BACKEND picks
# the display server that the toolkit uses. The script returns the exit
# status of the command and stops the session.
set -eu

if [ $# -eq 0 ]; then
    echo "usage: $0 COMMAND [ARGS...]" >&2
    exit 2
fi

# Start a private session bus first, and run the rest of the script in it.
if [ -z "${CHOSCORUI_IN_SESSION_BUS:-}" ]; then
    export CHOSCORUI_IN_SESSION_BUS=1
    exec dbus-run-session -- "$0" "$@"
fi

runtime=$(mktemp -d /tmp/choscorui-session.XXXXXX)
chmod 700 "$runtime"
export XDG_RUNTIME_DIR="$runtime"
pids=()
cleanup() {
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    rm -rf "$runtime"
}
trap cleanup EXIT

# The AT-SPI registry, where the distribution installs it.
for registry in /usr/libexec/at-spi2-registryd /usr/lib/at-spi2-core/at-spi2-registryd; do
    if [ -x "$registry" ]; then
        "$registry" --use-gnome-session=false >/dev/null 2>&1 &
        pids+=($!)
        break
    fi
done

# Sway with the headless backend and the pixman renderer, so it needs no
# GPU and no input devices.
# Sway refuses to run as root, which containers often are. It then runs
# as nobody; root clients can still open its socket.
as_compositor=()
if [ "$(id -u)" = 0 ]; then
    chown nobody "$runtime"
    as_compositor=(setpriv --reuid=nobody --regid=nogroup --clear-groups)
fi
cat >"$runtime/sway.conf" <<'CONF'
output HEADLESS-1 resolution 1280x800 position 0 0
default_border none
# Windows float, so each window keeps the size that it asks for.
for_window [app_id=".*"] floating enable
CONF
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
    WAYLAND_DISPLAY= DISPLAY= \
    "${as_compositor[@]}" sway --unsupported-gpu -c "$runtime/sway.conf" >"$runtime/sway.log" 2>&1 &
pids+=($!)

Xvfb :99 -screen 0 1280x800x24 +extension RANDR -nolisten tcp >"$runtime/xvfb.log" 2>&1 &
pids+=($!)

# Wait up to ten seconds for both servers.
for _ in $(seq 100); do
    socket=$(find "$runtime" -maxdepth 1 -name 'wayland-*' ! -name '*.lock' -printf '%f\n' | head -n 1)
    if [ -n "$socket" ] && [ -e /tmp/.X11-unix/X99 ]; then
        break
    fi
    sleep 0.1
done
if [ -z "${socket:-}" ]; then
    echo "headless-session: sway did not start" >&2
    cat "$runtime/sway.log" >&2
    exit 1
fi
if [ ! -e /tmp/.X11-unix/X99 ]; then
    echo "headless-session: Xvfb did not start" >&2
    cat "$runtime/xvfb.log" >&2
    exit 1
fi

export WAYLAND_DISPLAY="$socket"
export DISPLAY=:99
status=0
"$@" || status=$?
exit "$status"
