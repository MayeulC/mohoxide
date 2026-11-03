#!/bin/sh
# Fresh dist packaging time and runtime parity for two commits, on Linux
# (WSL on the Windows PC). Each commit gets its own worktree and empty
# target dir, is packaged with dev/package/package-linux.sh, then the two
# binaries run the 3000-tick fight in alternating rounds. Results:
# target/build-speed/results.txt.
#
#   dev/build-speed/measure-linux.sh [base] [head] [rounds]
#
# The fight needs the game: set RTS_GAME_DIR (e.g. the Windows install under
# /mnt/c) if src/install.rs doesn't find it.
set -eu
base=${1:-2ad97cb}
head=${2:-HEAD}
rounds=${3:-5}
repo=$(cd "$(dirname "$0")/../.." && pwd)
work="$repo/target/build-speed"
mkdir -p "$work"
results="$work/results.txt"
now() { date +%s.%N; }
secs() { awk "BEGIN { printf \"%.2f\", $2 - $1 }"; }

shas=""
for ref in "$base" "$head"; do
    sha=$(git -C "$repo" rev-parse --short "$ref")
    shas="$shas $sha"
    tree="$work/tree-$sha"
    git -C "$repo" worktree remove --force "$tree" 2>/dev/null || rm -rf "$tree"
    git -C "$repo" worktree prune
    git -C "$repo" worktree add --detach "$tree" "$sha" >/dev/null
    start=$(now)
    sh "$tree/dev/package/package-linux.sh" dist
    end=$(now)
    cp "$tree/target/package/dist/rts-core-engine" "$work/rts-$sha"
    echo "build $sha: $(secs "$start" "$end") s" | tee -a "$results"
done

i=1
while [ "$i" -le "$rounds" ]; do
    for sha in $shas; do
        log="$work/run-$sha-$i.log"
        start=$(now)
        (cd "$repo" && "$work/rts-$sha" --headless --ticks 3000 --script dev/scenes/fight.lua --log "$log" >/dev/null)
        end=$(now)
        sums=$(grep progress "$log" | grep -o 'checksum=[0-9a-f]*' | tr '\n' ' ')
        echo "run $i $sha: $(secs "$start" "$end") s $sums" | tee -a "$results"
    done
    i=$((i + 1))
done
