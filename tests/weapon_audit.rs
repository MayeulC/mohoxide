//! Golden test: the weapon-audit census — `dev/audit/targets.lua` +
//! `dev/audit/scenario.lua` + `dev/audit/weapon_audit.lua` (cat-joined as one script, the files' own
//! canonical route) run by `rts_core_engine::weapon_audit::census`: every
//! case in a session of its own on the flat audit map
//! (`tests/vault/maps/audit_duel`), its engagement midpoint at the map
//! centre — the very session `--audit-test N` runs — on a pool of threads,
//! merged in case-id order. One outcome row per case:
//! `id|attacker|weaponIdx|target|outcome|fired|damage`; a turreted weapon's
//! case is followed by its bearing sub-cases (`id@bearing`, the target at
//! that bearing off the attacker's facing; the gate set: 90, 180, -90 and
//! the bearing nearest a limited arc's centre). The rows must equal
//! `tests/golden/weapon_audit.txt` line for line (they carry the
//! fired/damage counts, so any behaviour flip shows). No case can observe
//! another, so a row is the same alone, in a subset or in the census.
//! Deterministic: seed 0, results independent of the thread count. Regenerate
//! an intended behaviour change and review the diff:
//!     UPDATE_GOLDEN=1 cargo test --test weapon_audit
//! Needs the FAF checkout (`../fa`) and game data; fails without them (`RTS_SKIP_DATA_TESTS=1` skips).
//! Run one: `cargo test --test weapon_audit audit`.

use rts_core_engine::weapon_audit::{self, Case, Paths};

const INIT_LUA_PATH: &str = "../fa/setup/bin/init_local_development.lua";
const FA_PATH: &str = "../Supreme Commander Forged Alliance";
const GOLDEN: &str = "tests/golden/weapon_audit.txt";
/// The repo's test vault holding the audit map (`dev/audit/flat_map.py`).
const VAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/vault");
const OUTCOMES: [&str; 8] = [
    "pass",
    "fired-no-hit",
    "held-no-fire",
    "no-target",
    "map-nosupport",
    "unsupported",
    "non-attack",
    "hit-no-damage",
];

fn have_data() -> bool {
    rts_core_engine::test_data::require_game("weapon audit")
}

fn outcome_totals(rows: &str) -> String {
    let mut by: std::collections::BTreeMap<&str, u32> = Default::default();
    for o in rows.lines().filter_map(|l| l.split('|').nth(4)) {
        *by.entry(o).or_default() += 1;
    }
    by.iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Structural invariants pinned before the golden compare: canonical ids
/// exactly 1..N in case order, each followed only by its own bearing
/// sub-cases (`id@bearing`, clockwise, never bearing 0), every row a 7-field
/// pipe row with a documented outcome and the right leading key, and a row
/// for every case.
fn check_invariants(cases: &[Case], rows: &[String]) {
    assert!(!cases.is_empty(), "the audit generated no cases");
    let mut canonical = 0u32;
    let mut last_sweep = 0;
    for (i, c) in cases.iter().enumerate() {
        if c.bearing == 0 {
            canonical += 1;
            last_sweep = 0;
            assert_eq!(c.id, canonical, "canonical case ids are not 1..N in case order: got {} at index {i}", c.id);
            assert_eq!(c.key, c.id.to_string(), "case {}'s key is {}", c.id, c.key);
        } else {
            let sweep = c.bearing.rem_euclid(360);
            assert!(
                c.id == canonical && sweep > last_sweep,
                "bearing sub-case {} at index {i} does not follow case {canonical}'s rows in clockwise order",
                c.key
            );
            last_sweep = sweep;
            assert_eq!(c.key, format!("{}@{}", c.id, c.bearing), "sub-case key");
        }
        assert!(
            !c.att_name.is_empty() && !c.weapon_name.is_empty(),
            "case {} is missing its human names (attName '{}', weaponName '{}')",
            c.id,
            c.att_name,
            c.weapon_name
        );
    }
    assert_eq!(
        rows.len(),
        cases.len(),
        "{} result rows for {} cases (the resolution drifted)",
        rows.len(),
        cases.len()
    );
    for (i, row) in rows.iter().enumerate() {
        let f: Vec<&str> = row.split('|').collect();
        assert_eq!(
            f.len(),
            7,
            "result row {} is not a 7-field pipe row: {row}",
            i + 1
        );
        assert!(
            OUTCOMES.contains(&f[4]),
            "result row {} outcome '{}' is not a documented outcome: {row}",
            i + 1,
            f[4]
        );
        assert_eq!(
            f[0],
            cases[i].key,
            "result row {} leads with a different key",
            i + 1
        );
    }
}

/// A structure's turret swings round to a target behind it before the first
/// shot: the audit turns bearing sub-case structures back to heading 0
/// after FAF's `RotateTowardsEnemy` has faced them at the map centre (that
/// is, at their target). With a 1-2 degree `FiringTolerance` the first shot
/// (`shot=`, ticks after the order) can't come before the swing (`slew=`).
/// Real game (FAF probe `probe_turretslew`): `uab2101` 9 ticks later at 180
/// than at 0.
fn check_turret_slew(cases: &[Case]) {
    let field = |aim: &str, name: &str| -> Option<u32> {
        aim.split_whitespace().find_map(|t| t.strip_prefix(name)).and_then(|v| v.parse().ok())
    };
    for key in ["64@180", "531@180"] {
        let c = cases.iter().find(|c| c.key == key).unwrap_or_else(|| panic!("no case {key}"));
        let (shot, slew) = (field(&c.aim, "shot="), field(&c.aim, "slew="));
        let (Some(shot), Some(slew)) = (shot, slew) else { panic!("case {key}: no shot / slew in '{}'", c.aim) };
        assert!(shot >= slew, "case {key} ({}): first shot {shot} ticks after the order, before its turret could swing round ({slew})", c.attacker);
    }
}

/// The full census: every weapon-bearing blueprint's engagement, each in its
/// own session, compared row for row with the golden.
#[test]
fn audit() {
    if !have_data() {
        return;
    }
    let started = std::time::Instant::now();
    let paths = Paths { init: INIT_LUA_PATH, game: FA_PATH, vault: std::path::Path::new(VAULT) };
    let threads = weapon_audit::threads();
    let census = weapon_audit::census(paths, None, threads, weapon_audit::GATE_BEARINGS)
        .unwrap_or_else(|e| panic!("weapon audit: {e}"));
    assert!(
        census.unfinished.is_empty(),
        "weapon audit: cases {:?} not done by their caps (budget + settle)",
        census.unfinished
    );
    assert!(
        census.failures.is_empty(),
        "weapon audit framework failed ({}):\n  {}",
        census.failures.len(),
        census.failures.join("\n  ")
    );
    let (cases, rows) = (census.cases, census.rows);
    check_invariants(&cases, &rows);
    check_turret_slew(&cases);
    let summary = outcome_totals(&rows.join("\n"));
    eprintln!(
        "audit: {} cases in {} sessions on {threads} threads (longest {} ticks, {} in all), {:.1}s wall: {summary}",
        cases.len(),
        census.sessions,
        census.max_ticks,
        census.total_ticks,
        started.elapsed().as_secs_f64()
    );
    let actual = rows.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(
            std::path::Path::new(GOLDEN)
                .parent()
                .expect("golden dir parent"),
        )
        .expect("create golden dir");
        std::fs::write(GOLDEN, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN} ({} rows)", cases.len());
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN)
        .expect("golden missing: run with UPDATE_GOLDEN=1 cargo test --test weapon_audit");
    if actual != expected {
        // Compared by row key (`id` / `id@bearing`), so an added or dropped
        // bearing row shows as itself, not as every row after it shifting.
        let keyed = |t: &str| -> std::collections::BTreeMap<String, String> {
            t.lines().map(|l| (l.split('|').next().unwrap_or("").to_owned(), l.to_owned())).collect()
        };
        let (old, new) = (keyed(&expected), keyed(&actual));
        let mut differing: Vec<String> = Vec::new();
        for l in expected.lines().chain(actual.lines()) {
            let k = l.split('|').next().unwrap_or("").to_owned();
            if old.get(&k) != new.get(&k) && !differing.contains(&k) {
                differing.push(k);
            }
        }
        let shown: String = differing
            .iter()
            .take(10)
            .map(|k| format!("\n  id {k}:\n    old: {}\n    new: {}", old.get(k).map_or("(none)", |s| s), new.get(k).map_or("(none)", |s| s)))
            .collect();
        panic!(
            "weapon audit diverged from {GOLDEN}: {} differing rows; first:{shown}\n  totals: old [{}] / new [{}]; rows old {} / new {}. Each case runs alone: `cargo run --release -- --headless --audit-test <id>` (or `<id>@<bearing>`) reproduces its row. Intended flip: UPDATE_GOLDEN=1 cargo test --test weapon_audit",
            differing.len(),
            outcome_totals(&expected),
            outcome_totals(&actual),
            expected.lines().count(),
            actual.lines().count()
        );
    }
}
