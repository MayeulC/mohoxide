#!/usr/bin/env bash
# Time each test suite separately, then the whole `cargo test`.
# Usage: dev/bench/test_suite_times.sh [outfile]
set -uo pipefail
cd "$(dirname "$0")/../.."

OUT="${1:-/tmp/suite_times.txt}"
: >"$OUT"

run() {
  local name="$1"; shift
  local start end
  start=$(python3 -c 'import time;print(time.time())')
  "$@" >/tmp/suite_out_$name.txt 2>&1
  local rc=$?
  end=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(f'{\"$name\":28s} {($end-$start):8.2f}s  rc=$rc')" | tee -a "$OUT"
}

# The main suites, one by one (cargo test runs the lib/bin tests together
# in one process per binary, so they are timed as cargo would run them).
run "lib"            cargo test --lib
run "bin"            cargo test --bin rts-core-engine
run "golden_fight"   cargo test --test golden_fight
run "scenarios"      cargo test --test scenarios
run "player_orders"  cargo test --test player_orders
run "ui"             cargo test --test ui
run "weapon_audit"   cargo test --test weapon_audit

run "ALL_cargo_test" cargo test --workspace --no-fail-fast

run "bench_compile"      cargo check --workspace --benches

echo "---- per-suite (cargo test -- --list / run times) ----"
grep -E "^test result|running [0-9]+ tests" /tmp/suite_out_*.txt 2>/dev/null | head -40
