#!/bin/zsh
# dev/audit/watch.sh: watch weapon-audit cases one after another, each in its
# own window (`--audit-test <id>`), camera on the case's engagement, the window
# closing by itself once the case's ticks have run (`--ticks`).
#
#   dev/audit/watch.sh 201 535 68     # these cases, in this order
#   dev/audit/watch.sh 531 531@180    # a case and a bearing sub-case of it (id@bearing)
#   SPEED=4 dev/audit/watch.sh        # every engaged case in the golden (~700 with bearings)
#
# Env: ZOOM (camera distance, default 100), SPEED (game speed, default 1),
# BIN (default target/release/rts-core-engine; build with cargo build --release),
# GOLDEN (default tests/golden/weapon_audit.txt; ids are its first column,
# bearing sub-cases included).
# Closing a window early moves on to the next case; Ctrl+C stops the loop.
# Failing cases only:
#   dev/audit/watch.sh $(awk -F'|' '$5!="pass" && $5!="non-attack" && $5!="unsupported" {print $1}' tests/golden/weapon_audit.txt)
set -u
cd "${0:A:h}/../.."
BIN=${BIN:-target/release/rts-core-engine}
GOLDEN=${GOLDEN:-tests/golden/weapon_audit.txt}
ZOOM=${ZOOM:-100}
SPEED=${SPEED:-1}
if [[ ! -x $BIN ]]; then
  echo "watch.sh: $BIN not found; run cargo build --release" >&2
  exit 2
fi
ids=("$@")
if (( ${#ids} == 0 )); then
  ids=(${(f)"$(awk -F'|' '$5!="non-attack" && $5!="unsupported" {print $1}' $GOLDEN)"})
fi
for id in $ids; do
  # "case 201: ueb4201[1] vs ueb2108 centred at 437, 512 (budget 660, 180 ticks, 0.4s)"
  line=$($BIN --headless --audit-test $id 2>/dev/null | head -1)
  if [[ $line != *"centred at"* ]]; then
    echo "case $id: no engagement to watch (${line:-no output})"
    continue
  fi
  echo $line
  x=$(sed -E 's/.*centred at ([0-9.-]+), ([0-9.-]+).*/\1/' <<< $line)
  z=$(sed -E 's/.*centred at ([0-9.-]+), ([0-9.-]+).*/\2/' <<< $line)
  t=$(sed -E 's/.* ([0-9]+) ticks,.*/\1/' <<< $line)
  $BIN --no-audio --audit-test $id --camera $x,$z,$ZOOM --ticks $((t + 30)) --speed $SPEED >/dev/null 2>&1
done
