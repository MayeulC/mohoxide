#!/bin/sh
# Runs a probe map in the real game under CrossOver and writes its log to
# dev/probes/out/<probe>.fa.log. Usage: dev/probes/run_fa.sh probe_fight [seconds]
set -e
probe=${1:-probe_fight}
secs=${2:-90}
here=$(cd "$(dirname "$0")" && pwd)
game="$here/../../../Supreme Commander Forged Alliance"
cx="$HOME/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin"
bottle=${BOTTLE:-Steam}
mkdir -p "$here/out"
winpath() { echo "Z:$1" | tr '/' '\\'; }
log="$here/out/$probe.fa.log"
rm -f "$log"
"$cx/wine" --bottle "$bottle" "$(winpath "$game/bin/SupremeCommander.exe")" \
    /init "$(winpath "$here/init_probe.lua")" /map "$probe" /log "$(winpath "$log")" \
    /nosound /nomovie /exitongameover &
pid=$!
# Stop once the probe reports done (or after the timeout).
i=0
while [ $i -lt "$secs" ]; do
    sleep 1; i=$((i + 1))
    if [ -f "$log" ] && grep -q "PROBE done" "$log"; then break; fi
done
pkill -f SupremeCommander.exe 2>/dev/null || true; wait $pid 2>/dev/null || true
grep -c "PROBE" "$log" 2>/dev/null || echo "no PROBE lines in $log"
