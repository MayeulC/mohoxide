#!/usr/bin/env bash
# Plays the scenario tests (tests/scenarios/*.lua) one after another in a
# window, for recording, the camera following their units. Each runs until
# it calls done() (plus a short hold), then EndGame() closes the window and
# the next one starts.
#
#   dev/show_scenarios.sh                 # all of them, in tests/scenarios.rs order
#   dev/show_scenarios.sh transport fuel  # just these
#   SPEED=2 ZOOM=60 KEEP=1 dev/show_scenarios.sh
# The Lua appended to each is dev/show_scenarios.lua; on Windows use
# dev/show_scenarios.ps1.
set -u
cd "$(dirname "$0")/.."

SPEED=${SPEED:-1}
ZOOM=${ZOOM:-80}
HOLD=${HOLD:-30}     # ticks to keep showing after done()
WORK=$(mktemp -d)
[ -n "${KEEP:-}" ] && echo "keeping $WORK" || trap 'rm -rf "$WORK"' EXIT

cargo build --release || exit 1

if [ $# -gt 0 ]; then
    list=$(for n in "$@"; do grep -o "scenario(\"$n\", [0-9]*)" tests/scenarios.rs || echo "scenario(\"$n\", 3000)"; done)
else
    list=$(grep -o 'scenario("[a-z_0-9]*", [0-9]*)' tests/scenarios.rs)
fi

echo "$list" | sed -E 's/scenario\("([^"]+)", ([0-9]+)\)/\1 \2/' | while read -r name max; do
    src="tests/scenarios/$name.lua"
    [ -f "$src" ] || { echo "skip $name: no $src"; continue; }
    # Camera on the first unit the scenario places with literal coordinates
    # (CreateUnitHPR(bp, army, x, y, z, ...)), else the scenario land.
    read -r cx cz < <(grep -oE "CreateUnitHPR\('[a-z0-9]+', *'ARMY_[0-9]', *[0-9.]+, *[^,]+, *[0-9.]+" "$src" \
        | head -1 | awk -F, '{gsub(/ /,""); print $3, $5}')
    cx=${cx:-420}; cz=${cz:-600}
    wrapped="$WORK/$name.lua"
    cat "$src" > "$wrapped"
    { printf '\nSHOW = { name = "%s", zoom = %s, hold = %s }\n' "$name" "$ZOOM" "$HOLD"
      cat dev/show_scenarios.lua; } >> "$wrapped"
    echo "== $name (camera $cx,$cz, at most $max ticks)"
    ./target/release/rts-core-engine --session --no-ai --no-record --no-audio --speed "$SPEED" \
        --ticks $((max + HOLD + 10)) --camera "$cx,$cz,$ZOOM" \
        --script "$wrapped" --log "$WORK/game.log" || echo "   exit $?"
done
