//! The weapon-audit census runner, shared by the CLI flags (`--audit-table`,
//! `--audit-test`, `src/audit.rs`) and the golden test
//! (`tests/weapon_audit.rs`).
//!
//! Every case runs in a session of its own: `dev/audit/targets.lua` +
//! `dev/audit/scenario.lua` + `dev/audit/weapon_audit.lua` with `WEAPON_AUDIT_CASE = id`, the case
//! alone on the flat audit map, its engagement midpoint at the map centre —
//! exactly the `--audit-test` session. Nothing of one case (units, shots,
//! splash, the sim's per-tick budgets) can reach another, so a case's row is
//! the same whether it runs alone, in a subset or in the whole census. One
//! short plan session lists the cases first: the statically known ones
//! (non-attack: death / teleport / dummy weapons; unsupported: anti-
//! projectile, ...) resolve there without units; the
//! rest are run on a pool of threads, longest budget first, and merged back
//! in case-id order (results never depend on which thread ran what). A
//! turreted weapon's case also has bearing sub-cases (`id@bearing`, the target
//! at that bearing off the attacker's facing, each its own session, merged
//! clockwise after the canonical row); `WEAPON_AUDIT_BEARINGS` picks the set
//! the plan lists ([`GATE_BEARINGS`] for the golden).

use mlua::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::debug_ui::DebugUiState;
use crate::engine;

/// The audit's map: flat 81 km land / water bands (`dev/audit/flat_map.py`),
/// kept in the repo's test vault (`tests/vault`).
pub const MAP: &str = "/maps/audit_duel/audit_duel_scenario.lua";
/// Cap while the script is still generating (`AUDIT_CASES` unreadable yet);
/// once it exists the cap is the max case budget + `SETTLE` ticks.
const SAFETY_CAP: u32 = 4000;
const SETTLE: u32 = 200;

/// One case's table entry (`_G.AUDIT_CASES[i]`).
#[derive(Clone, Debug)]
pub struct Case {
    /// The weapon's case number (1..N, stable: the sorted blueprint walk).
    pub id: u32,
    /// The row id: `id` for the canonical case (target straight ahead),
    /// `id@bearing` for a bearing sub-case of it.
    pub key: String,
    /// The target's bearing off the attacker's facing, degrees (0: the
    /// canonical case; positive: the side positive `HeadingArcCenter` points).
    pub bearing: i32,
    /// The aim evidence (`aim b=.. turn=.. arc=.. n=.. lat=.. long=..
    /// max=..`), empty when the case has none.
    pub aim: String,
    pub attacker: String,
    pub weapon: u32,
    pub target: String,
    pub x: f64,
    pub z: f64,
    pub budget: u32,
    pub att_name: String,
    pub weapon_name: String,
    pub tgt_name: String,
    pub reason: String,
    pub reason_key: String,
}

/// What one audit session left behind.
#[derive(Debug, Default)]
pub struct Session {
    /// `ScenarioResult.done` was reached under the cap.
    pub done: bool,
    pub cases: Vec<Case>,
    /// `_G.AUDIT_RESULTS`: `id|attacker|weaponIdx|target|outcome|fired|damage`.
    pub rows: Vec<String>,
    /// The script's own failures (`ScenarioResult.failures`).
    pub failures: Vec<String>,
    pub notes: Vec<String>,
    /// The plan session's cases still to run (`_G.AUDIT_PENDING`): key, budget.
    pub pending: Vec<(String, u32)>,
    pub ticks: u32,
}

/// The whole census (or a subset), merged in case-id order.
#[derive(Debug, Default)]
pub struct Census {
    pub cases: Vec<Case>,
    pub rows: Vec<String>,
    /// Every session's script failures, prefixed with the case id.
    pub failures: Vec<String>,
    /// Cases (keys) whose session did not finish under its cap.
    pub unfinished: Vec<String>,
    /// Sessions run (the plan session included).
    pub sessions: usize,
    /// The longest session's ticks.
    pub max_ticks: u32,
    /// Ticks summed over every session.
    pub total_ticks: u64,
}

/// Where the engine finds its files: FAF's init script, the game folder and
/// the vault holding the audit map.
#[derive(Clone, Copy, Debug)]
pub struct Paths<'a> {
    pub init: &'a str,
    pub game: &'a str,
    pub vault: &'a Path,
}

/// Is `key` a case key: `n` or `n@bearing` (bearing in degrees, may be
/// negative)?
pub fn is_case_key(key: &str) -> bool {
    let (id, bearing) = match key.split_once('@') {
        Some((id, b)) => (id, Some(b)),
        None => (key, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(id) && bearing.is_none_or(|b| digits(b.strip_prefix('-').unwrap_or(b)))
}

/// The selection prelude for a single case (`key`: `n` or `n@bearing`).
pub fn case_prelude(key: &str) -> String {
    assert!(is_case_key(key), "not a case key: {key}");
    format!("rawset(_G, 'WEAPON_AUDIT_CASE', '{key}')")
}

/// The bearing set the plan lists (`WEAPON_AUDIT_BEARINGS`, read by
/// `dev/audit/weapon_audit.lua`): `gate` (90, 180, -90 and the bearing
/// nearest a limited arc's centre; the golden's set), `all` (every 45
/// degrees), `none`, or a list such as `90,135`.
pub const GATE_BEARINGS: &str = "gate";

/// `WEAPON_AUDIT_BEARINGS` from the environment, else the gate set.
pub fn bearings() -> String {
    std::env::var("WEAPON_AUDIT_BEARINGS").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| GATE_BEARINGS.into())
}

/// Clockwise order of a bearing (0 first): a weapon's rows sort by it.
fn sweep(bearing: i32) -> i32 {
    bearing.rem_euclid(360)
}

/// A temp-script tag for a case key (`46@-90` -> `46b-90`).
pub fn key_tag(key: &str) -> String {
    key.replace('@', "b")
}

/// Cat-joins the optional prelude, `dev/audit/targets.lua`,
/// `dev/audit/scenario.lua` and `dev/audit/weapon_audit.lua` (read from the working directory, the engine
/// checkout) into a temp script named by `tag`, unique per process.
pub fn script(tag: &str, prelude: Option<&str>) -> Result<String, String> {
    let mut content = String::new();
    if let Some(pre) = prelude {
        content.push_str(pre);
        content.push('\n');
    }
    for file in ["dev/audit/targets.lua", "dev/audit/scenario.lua", "dev/audit/weapon_audit.lua"] {
        content.push_str(&std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?);
        content.push('\n');
    }
    let path = std::env::temp_dir().join(format!("rts_audit_{tag}_{}.lua", std::process::id()));
    std::fs::write(&path, content).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The audit session config: the flat audit map, fog none (the engagement
/// search must see targets), no AI, no initial units, the combined script.
pub fn session_config(script: String) -> engine::session::SessionConfig {
    engine::session::SessionConfig {
        scenario: MAP.into(),
        script: Some(script),
        ticks: Some(0),
        ai: false,
        create_initial_units: false,
        fog_of_war: Some("none".into()),
        ..Default::default()
    }
}

fn into_u32(v: LuaValue) -> Option<u32> {
    match v {
        LuaValue::Integer(n) => u32::try_from(n).ok(),
        LuaValue::Number(n) if n >= 0.0 && n.fract() == 0.0 => u32::try_from(n as u64).ok(),
        _ => None,
    }
}

/// The script's case table (`_G.AUDIT_CASES`), None until it is set.
fn audit_cases(lua: &Lua) -> Option<Vec<Case>> {
    let t: Option<LuaTable> = lua.globals().raw_get("AUDIT_CASES").ok().flatten();
    let text = |e: &LuaTable, k: &str| e.raw_get::<Option<String>>(k).ok().flatten().unwrap_or_default();
    t.and_then(|t| {
        t.sequence_values::<LuaTable>()
            .map(|e| {
                let e = e.ok()?;
                let id = into_u32(e.raw_get("id").ok()?)?;
                Some(Case {
                    id,
                    key: e.raw_get::<Option<String>>("key").ok().flatten().unwrap_or_else(|| id.to_string()),
                    bearing: e.raw_get::<Option<i32>>("bearing").ok().flatten().unwrap_or(0),
                    aim: text(&e, "aim"),
                    attacker: e.raw_get("attacker").ok()?,
                    weapon: into_u32(e.raw_get("weapon").ok()?)?,
                    target: e.raw_get::<Option<String>>("target").ok().flatten().unwrap_or_else(|| "-".into()),
                    x: e.raw_get("x").ok().unwrap_or(0.0),
                    z: e.raw_get("z").ok().unwrap_or(0.0),
                    budget: into_u32(e.raw_get("budget").ok()?)?,
                    att_name: text(&e, "attName"),
                    weapon_name: text(&e, "weaponName"),
                    tgt_name: text(&e, "tgtName"),
                    reason: text(&e, "reason"),
                    reason_key: text(&e, "reasonKey"),
                })
            })
            .collect::<Option<Vec<_>>>()
    })
}

fn strings(t: Option<LuaTable>) -> Vec<String> {
    t.map(|t| t.sequence_values::<String>().filter_map(Result::ok).collect()).unwrap_or_default()
}

fn scenario_result(lua: &Lua) -> Option<LuaTable> {
    lua.globals().raw_get::<Option<LuaTable>>("ScenarioResult").ok().flatten()
}

/// One audit session (the combined script at `script`), stepped to
/// `ScenarioResult.done` under the caps (the static ceiling before
/// `AUDIT_CASES` exists, then max budget + settle). `Err` = engine/session
/// breakage; the script's own reports are the result's `failures`.
pub fn run_session(paths: Paths, script: &str) -> Result<Session, String> {
    let mut engine =
        engine::Engine::new_with_vault(paths.init, paths.game, Arc::new(Mutex::new(DebugUiState::new())), Some(paths.vault))
            .map_err(|e| format!("engine init: {e}"))?;
    engine.start(&session_config(script.into())).map_err(|e| format!("session: {e}"))?;
    let mut ticks = 0u32;
    let mut done = false;
    let mut cap = SAFETY_CAP;
    let mut capped = false;
    while ticks < cap {
        engine.tick().map_err(|e| format!("tick {ticks}: {e}"))?;
        ticks += 1;
        let lua = engine.sim_lua();
        if scenario_result(lua).is_some_and(|r| r.get::<bool>("done").unwrap_or(false)) {
            done = true;
            break;
        }
        // The budgets are fixed once the case table exists: read it once.
        if !capped && let Some(cases) = audit_cases(lua) {
            cap = cases.iter().map(|c| c.budget).max().unwrap_or(0) + SETTLE;
            capped = true;
        }
    }
    let lua = engine.sim_lua();
    let r = scenario_result(lua);
    let field = |k: &str| r.as_ref().and_then(|r| r.get::<LuaTable>(k).ok());
    let pending: Option<LuaTable> = lua.globals().raw_get("AUDIT_PENDING").ok().flatten();
    Ok(Session {
        done,
        cases: audit_cases(lua).unwrap_or_default(),
        rows: strings(lua.globals().raw_get("AUDIT_RESULTS").ok().flatten()),
        failures: strings(field("failures")),
        notes: strings(field("notes")),
        pending: pending
            .map(|t| {
                t.sequence_values::<LuaTable>()
                    .filter_map(|e| {
                        let e = e.ok()?;
                        Some((e.raw_get::<String>("key").ok()?, into_u32(e.raw_get("budget").ok()?)?))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ticks,
    })
}

/// The threads to run case sessions on: `WEAPON_AUDIT_THREADS`, else the
/// machine's parallelism.
pub fn threads() -> usize {
    std::env::var("WEAPON_AUDIT_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
}

/// The census over `ids` (None = every case; a bare id selects the case and
/// its listed bearings, `id@bearing` one bearing) with the `bearings` set
/// (see [`GATE_BEARINGS`]): the plan session, then one session per remaining
/// case on `threads` threads, merged by case id then clockwise bearing.
/// `Err` = engine breakage or a bad selection (the plan's failure text).
pub fn census(paths: Paths, ids: Option<&[String]>, threads: usize, bearings: &str) -> Result<Census, String> {
    if let Some(bad) = ids.into_iter().flatten().find(|k| !is_case_key(k)) {
        return Err(format!("not a case key: '{bad}' (n or n@bearing)"));
    }
    if !bearings.bytes().all(|b| b.is_ascii_alphanumeric() || b",- ".contains(&b)) {
        return Err(format!("bad bearing set '{bearings}'"));
    }
    let selection = ids.map(|ids| ids.join(","));
    let mut prelude = String::from("rawset(_G, 'WEAPON_AUDIT_PLAN', true)\n");
    prelude.push_str(&format!("rawset(_G, 'WEAPON_AUDIT_BEARINGS', '{bearings}')\n"));
    if let Some(sel) = &selection {
        prelude.push_str(&format!("rawset(_G, 'WEAPON_AUDIT_IDS', '{sel}')\n"));
    }
    let plan = run_session(paths, &script("plan", Some(&prelude))?)?;
    if !plan.done || !plan.failures.is_empty() {
        return Err(format!("the plan session failed: {}", plan.failures.join("; ")));
    }
    let mut census = Census { sessions: 1, max_ticks: plan.ticks, total_ticks: plan.ticks as u64, ..Default::default() };
    let mut merged: Vec<(Case, String)> = plan.cases.into_iter().zip(plan.rows).collect();

    // Longest budget first: the pool's tail is then short cases.
    let mut pending = plan.pending.clone();
    pending.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let order: Vec<String> = pending.into_iter().map(|(key, _)| key).collect();
    let scripts = order
        .iter()
        .map(|key| script(&format!("case{}", key_tag(key)), Some(&case_prelude(key))))
        .collect::<Result<Vec<_>, _>>()?;
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<Result<Session, String>>>> = order.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, order.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= order.len() {
                        break;
                    }
                    *results[i].lock().unwrap() = Some(run_session(paths, &scripts[i]));
                }
            });
        }
    });
    for (id, result) in order.iter().zip(results) {
        let session = result.into_inner().unwrap().unwrap_or_else(|| Err("not run".into()));
        let session = session.map_err(|e| format!("case {id}: {e}"))?;
        census.sessions += 1;
        census.max_ticks = census.max_ticks.max(session.ticks);
        census.total_ticks += session.ticks as u64;
        census.failures.extend(session.failures.iter().map(|f| format!("case {id}: {f}")));
        if !session.done {
            census.unfinished.push(id.clone());
        }
        match (session.cases.into_iter().next(), session.rows.into_iter().next()) {
            (Some(c), Some(r)) if &c.key == id => merged.push((c, r)),
            _ if !session.done => {}
            _ => census.failures.push(format!("case {id}: its session left no case row")),
        }
    }
    merged.sort_by_key(|(c, _)| (c.id, sweep(c.bearing)));
    census.unfinished.sort_unstable();
    (census.cases, census.rows) = merged.into_iter().unzip();
    Ok(census)
}
