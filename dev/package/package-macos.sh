#!/bin/sh
# Builds the macOS download: dist/rts-core-engine-<version>-macos.zip, holding
# "RTS Core Engine.app" (one binary for Apple silicon and Intel, ad-hoc signed),
# README.txt, LICENSE.txt and THIRD-PARTY-LICENSES.txt. See docs/guides/packaging.md.
#
#   dev/package/package-macos.sh [dist|release] [universal|native]
set -eu
profile=${1:-dist}
arches=${2:-universal}
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
commit=$(git rev-parse --short HEAD)
name="rts-core-engine-$version-$commit-macos"
export CARGO_TARGET_DIR="$repo/target/package"
# Package with the default toolchain linker even where a user config links
# with ld64.lld (docs/guides/running.md): with --target, build scripts (host
# artifacts) get that linker but not the target's `-C linker-flavor=ld64.lld`
# rustflag, so they hand lld cc-style arguments and fail. RUSTFLAGS replaces
# config rustflags outright (an empty per-target env var would be merged with
# them, not replace them).
export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=cc
export RUSTFLAGS=""

if [ "$arches" = universal ]; then
    targets="aarch64-apple-darwin x86_64-apple-darwin"
    rustup target add $targets >/dev/null
else
    targets=$(rustc -vV | sed -n 's/^host: //p')
fi
bins=""
target_args=""
for t in $targets; do
    target_args="$target_args --target $t"
done
# One cargo invocation for all arches: one dependency DAG, one jobserver, so
# rustc processes float across cores instead of each arch owning half.
MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --profile "$profile" --bin rts-core-engine $target_args
for t in $targets; do
    bins="$bins $CARGO_TARGET_DIR/$t/$profile/rts-core-engine"
done

out="$repo/dist/$name"
app="$out/RTS Core Engine.app"
rm -rf "$out" "$out.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# shellcheck disable=SC2086
lipo -create -output "$app/Contents/MacOS/rts-core-engine" $bins
cp LICENSE "$app/Contents/Resources/LICENSE.txt"
cp dev/package/THIRD-PARTY-LICENSES.txt "$app/Contents/Resources/THIRD-PARTY-LICENSES.txt"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.rts-core-engine</string>
  <key>CFBundleName</key><string>RTS Core Engine</string>
  <key>CFBundleDisplayName</key><string>RTS Core Engine</string>
  <key>CFBundleExecutable</key><string>rts-core-engine</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Thomas Monkman; GNU GPL v3 or later, see LICENSE.txt</string>
  <key>NSLocalNetworkUsageDescription</key><string>LAN games: finding and joining games on your local network.</string>
</dict>
</plist>
PLIST
# Ad-hoc signature: Apple silicon won't run unsigned code at all. Without a
# Developer ID and notarisation, Gatekeeper still asks on first open
# (README.txt says how).
codesign --force --deep --sign - "$app"
cp dev/package/README.txt "$out/README.txt"
cp LICENSE "$out/LICENSE.txt"
cp dev/package/THIRD-PARTY-LICENSES.txt "$out/THIRD-PARTY-LICENSES.txt"
(cd "$repo/dist" && ditto -c -k --keepParent "$name" "$name.zip")
echo "$out.zip"
