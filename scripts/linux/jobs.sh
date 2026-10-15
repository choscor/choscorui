#!/usr/bin/env bash
# The Linux CI jobs. scripts/linux/run-in-container.sh runs them in the
# image of scripts/linux/Dockerfile; on a Linux machine with the packages
# of CONTRIBUTING.md, run them directly from the repository root.
#
#   scripts/linux/jobs.sh check    # docs, format, lint, MSRV, and tests
#   scripts/linux/jobs.sh native   # smoke tests, gallery, and autopilot
set -eu

cd "$(dirname "$0")/../.."
exclude=(--exclude choscorui-windows --exclude choscorui-macos)
session=scripts/linux/headless-session.sh

check() {
    scripts/check-docs.sh
    cargo fmt --all --check
    cargo clippy --workspace "${exclude[@]}" --all-targets --locked -- -D warnings
    cargo +1.88 check --workspace "${exclude[@]}" --all-targets --locked
    cargo test --workspace "${exclude[@]}" --locked
}

# The smoke tests of one display server and painter. Native windows open,
# so each test runs alone.
smoke() {
    local backend=$1 painter=$2
    echo "== smoke: CHOSCORUI_BACKEND=$backend CHOSCORUI_PAINTER=$painter"
    export CHOSCORUI_BACKEND=$backend CHOSCORUI_PAINTER=$painter CHOSCORUI_RUN_LINUX_SMOKE=1
    $session cargo test -p choscorui-linux --test window_smoke --locked
    $session cargo test -p choscorui-linux --lib --locked wayland_session_
    $session cargo test -p choscorui-linux --test clipboard_smoke --locked -- --test-threads=1
    $session cargo test -p choscorui-linux --test ime_smoke --locked -- --test-threads=1
    $session cargo test -p choscorui-linux --test modal_smoke --locked -- --test-threads=1
    $session cargo test -p choscorui --test app_smoke --locked
    $session cargo test -p choscorui --test dialog_smoke --locked -- --test-threads=1
    $session cargo test -p choscorui --test errors_and_capabilities --locked
    $session env CHOSCORUI_GALLERY_AUTOCLOSE_MS=1000 cargo run -p choscorui-gallery --locked
    unset CHOSCORUI_BACKEND CHOSCORUI_PAINTER CHOSCORUI_RUN_LINUX_SMOKE
}

native() {
    cargo build -p choscorui-gallery --locked
    for backend in wayland x11; do
        for painter in gles cairo; do
            smoke "$backend" "$painter"
        done
    done
    # GLES paints like the software renderer, and text like Cairo.
    CHOSCORUI_RUN_LINUX_SMOKE=1 $session cargo test -p choscorui --test gles_parity_smoke --locked
    # Without a display server, the app reports Unsupported.
    env -u WAYLAND_DISPLAY -u DISPLAY CHOSCORUI_BACKEND=x11 \
        cargo test -p choscorui --test errors_and_capabilities --locked
    for backend in wayland x11; do
        echo "== autopilot: CHOSCORUI_BACKEND=$backend"
        CHOSCORUI_BACKEND=$backend $session env \
            CHOSCORUI_GALLERY_SETTINGS="$(mktemp -d)/gallery.txt" CHOSCORUI_GALLERY_AUTOPILOT=1 \
            cargo run -p choscorui-gallery --locked
    done
}

case "${1:-}" in
    check) check ;;
    native) native ;;
    *)
        echo "usage: $0 check|native" >&2
        exit 2
        ;;
esac
