#!/usr/bin/env bash
# Runs a Linux CI job in the image of scripts/linux/Dockerfile.
#
#   scripts/linux/run-in-container.sh check    # the linux-check job
#   scripts/linux/run-in-container.sh native   # the linux-native job
#   scripts/linux/run-in-container.sh shell    # an interactive shell
#   scripts/linux/run-in-container.sh -- CMD   # any command, in a session
#
# The repository is mounted at /work. The target directory and the Cargo
# registry live in named volumes, so they survive between runs and do not
# mix with the macOS target directory.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
image=${CHOSCORUI_LINUX_IMAGE:-choscorui-linux-ci}

docker build -q -t "$image" "$root/scripts/linux" >/dev/null

job=${1:-check}
[ $# -gt 0 ] && shift
case "$job" in
    check) command=(scripts/linux/jobs.sh check) ;;
    native) command=(scripts/linux/jobs.sh native) ;;
    shell) command=(bash) ;;
    --) command=(scripts/linux/headless-session.sh "$@") ;;
    *)
        echo "usage: $0 check|native|shell|-- COMMAND" >&2
        exit 2
        ;;
esac

tty=()
[ -t 0 ] && tty=(-it)
exec docker run --rm "${tty[@]}" \
    -v "$root:/work" \
    -v choscorui-linux-target:/work/target \
    -v choscorui-linux-cargo:/usr/local/cargo/registry \
    -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
    -e RUSTFLAGS="${RUSTFLAGS:--D warnings}" \
    "$image" "${command[@]}"
