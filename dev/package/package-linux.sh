#!/bin/sh
# Builds the Linux download: dist/rts-core-engine-<version>-<commit>-linux-x64.tar.gz,
# holding rts-core-engine, README.txt, LICENSE.txt and THIRD-PARTY-LICENSES.txt.
# Run on Linux (WSL on the Windows PC works). See docs/guides/packaging.md.
#
#   dev/package/package-linux.sh [dist|release]
set -eu
profile=${1:-dist}
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
commit=$(git rev-parse --short HEAD)
name="rts-core-engine-$version-$commit-linux-x64"
export CARGO_TARGET_DIR="$repo/target/package"

cargo build --profile "$profile" --bin rts-core-engine

out="$repo/dist/$name"
rm -rf "$out" "$out.tar.gz"
mkdir -p "$out"
cp "$CARGO_TARGET_DIR/$profile/rts-core-engine" "$out/rts-core-engine"
cp dev/package/README.txt "$out/README.txt"
cp LICENSE "$out/LICENSE.txt"
cp dev/package/THIRD-PARTY-LICENSES.txt "$out/THIRD-PARTY-LICENSES.txt"
tar -czf "$out.tar.gz" -C "$repo/dist" "$name"
echo "$out.tar.gz"
