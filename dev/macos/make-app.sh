#!/bin/sh
# Wraps the built engine in a minimal .app so macOS treats it as its own app:
# it asks for Local Network access for "RTS Core Engine" (NSLocalNetworkUsageDescription)
# instead of charging the launching terminal/editor, which may not be allowed.
#
#   dev/macos/make-app.sh [debug|release]      -> target/<profile>/RTS Core Engine.app
#   open -n "target/debug/RTS Core Engine.app" --args --log game1.log
#
# The launcher cds to the repo (the engine reads ../fa etc. relative to it)
# and execs the binary, so --log paths are relative to the repo too.
set -eu
profile=${1:-debug}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/target/$profile/RTS Core Engine.app"
[ -x "$repo/target/$profile/rts-core-engine" ] || { echo "build the $profile binary first" >&2; exit 1; }
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.rts-core-engine.$profile</string>
  <key>CFBundleName</key><string>RTS Core Engine</string>
  <key>CFBundleExecutable</key><string>launch</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>LAN games: finding and joining games on your local network.</string>
</dict>
</plist>
PLIST
cat > "$app/Contents/MacOS/launch" <<LAUNCH
#!/bin/sh
cd "$repo" && exec "$repo/target/$profile/rts-core-engine" "\$@"
LAUNCH
chmod +x "$app/Contents/MacOS/launch"
codesign --force --sign - "$app" >/dev/null 2>&1 || true
echo "$app"
