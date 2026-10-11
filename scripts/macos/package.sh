#!/bin/sh
# Builds the gallery's release app bundle, signs it, optionally notarizes
# it, and writes dist/choscorui-<version>/ (the app, install.sh,
# uninstall.sh, README.md, licenses, and THIRD-PARTY-NOTICES.txt) and a zip
# of it with a SHA-256 checksum.
#
# Environment:
#   CHOSCORUI_SIGN_IDENTITY  codesign identity ("Developer ID Application: …").
#                            Unset: ad-hoc signature (runs locally, not
#                            distributable).
#   CHOSCORUI_NOTARY_PROFILE notarytool keychain profile (see docs/releasing.md).
#                            Set together with a Developer ID identity to
#                            notarize and staple.
#   CHOSCORUI_UNIVERSAL=1    build arm64 + x86_64 and merge with lipo
#                            (needs `rustup target add x86_64-apple-darwin
#                            aarch64-apple-darwin`).
#   CHOSCORUI_DIST           output directory (default: dist).
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
dist=${CHOSCORUI_DIST:-"$root/dist"}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/choscorui/Cargo.toml" | head -n 1)
identity=${CHOSCORUI_SIGN_IDENTITY:--}
arch=$(uname -m)
[ "${CHOSCORUI_UNIVERSAL:-0}" = 1 ] && arch=universal

# name | cargo package | manifest directory | bundle id | executable | resources
apps="Gallery|choscorui-gallery|.|com.choscor.gallery|choscorui-gallery|gallery/resources"

build() { # package manifest-dir -> path of the built binary
    package=$1
    dir=$root/$2
    if [ "${CHOSCORUI_UNIVERSAL:-0}" = 1 ]; then
        for target in aarch64-apple-darwin x86_64-apple-darwin; do
            (cd "$dir" && cargo build --quiet --release --locked -p "$package" --target "$target")
        done
        target_dir=$dir/target
        lipo -create -output "$target_dir/release-universal-$package" \
            "$target_dir/aarch64-apple-darwin/release/$package" \
            "$target_dir/x86_64-apple-darwin/release/$package"
        echo "$target_dir/release-universal-$package"
    else
        (cd "$dir" && cargo build --quiet --release --locked -p "$package")
        echo "$dir/target/release/$package"
    fi
}

plist() { # name bundle-id executable
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>choscorui $1</string>
    <key>CFBundleExecutable</key><string>$3</string>
    <key>CFBundleIdentifier</key><string>$2</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>choscorui $1</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>15.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSSupportsAutomaticTermination</key><false/>
</dict>
</plist>
EOF
}

stage="$dist/choscorui-$version"
rm -rf "$stage"
mkdir -p "$stage"
echo "$apps" | while IFS='|' read -r name package dir bundle_id executable resources; do
    binary=$(build "$package" "$dir")
    app="$stage/choscorui $name.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/$executable"
    plist "$name" "$bundle_id" "$executable" >"$app/Contents/Info.plist"
    printf 'APPL????' >"$app/Contents/PkgInfo"
    if [ -n "$resources" ]; then
        cp -R "$root/$resources/." "$app/Contents/Resources/"
    fi
    # Hardened runtime and a secure timestamp are required for
    # notarization; ad-hoc signatures cannot use a timestamp.
    if [ "$identity" = - ]; then
        codesign --force --sign - "$app"
    else
        codesign --force --options runtime --timestamp --sign "$identity" "$app"
    fi
    codesign --verify --strict "$app"
    echo "built $app"
done

cp "$root/scripts/macos/install.sh" "$root/scripts/macos/uninstall.sh" "$stage/"
cp "$root/scripts/macos/archive-readme.md" "$stage/README.md"
# The project licenses and the notices of the statically linked
# third-party code travel with the binaries.
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$stage/"
cp "$root/scripts/macos/third-party-notices.txt" "$stage/THIRD-PARTY-NOTICES.txt"

archive="$dist/choscorui-$version-macos-$arch.zip"
pack() {
    rm -f "$archive"
    (cd "$dist" && ditto -c -k --keepParent "choscorui-$version" "$archive")
}
pack

if [ -n "${CHOSCORUI_NOTARY_PROFILE:-}" ]; then
    if [ "$identity" = - ]; then
        echo "notarization needs a Developer ID identity (CHOSCORUI_SIGN_IDENTITY)" >&2
        exit 1
    fi
    xcrun notarytool submit "$archive" --keychain-profile "$CHOSCORUI_NOTARY_PROFILE" --wait
    for app in "$stage"/*.app; do
        xcrun stapler staple "$app"
        spctl --assess --type execute --verbose "$app"
    done
    # Re-archive so the stapled tickets ship inside the bundles.
    pack
fi

(cd "$dist" && shasum -a 256 "$(basename "$archive")" >"$(basename "$archive").sha256")
echo "wrote $archive"
