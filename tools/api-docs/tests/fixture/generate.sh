#!/usr/bin/env bash
# Writes the rustdoc JSON of the api-docs test fixture with the pinned
# nightly. Run it after a bump of tools/api-docs/nightly.txt and
# FORMAT_VERSION, then run `cargo test -p choscorui-api-docs`.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
pin=$(grep -v '^#' "$here/../../nightly.txt" | head -n 1)
work=$(mktemp -d "${TMPDIR:-/tmp}/api-docs-fixture.XXXXXX")
trap 'rm -rf "$work"' EXIT

rustc "+$pin" --edition 2024 --crate-type lib --crate-name choscorui_view \
    --out-dir "$work" "$here/view.rs"
rustdoc "+$pin" --edition 2024 --crate-type lib --crate-name choscorui_view \
    -Z unstable-options --output-format json --out-dir "$work" "$here/view.rs"
rustdoc "+$pin" --edition 2024 --crate-type lib --crate-name choscorui \
    --extern "choscorui_view=$work/libchoscorui_view.rlib" \
    -Z unstable-options --output-format json --out-dir "$work" "$here/facade.rs"
cp "$work/choscorui_view.json" "$work/choscorui.json" "$here/"
