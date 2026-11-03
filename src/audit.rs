//! The weapon-audit CLI flags (`--audit-table`, `--audit-test`): the census
//! runner is the library's `weapon_audit` (shared with
//! `tests/weapon_audit.rs`): every case in a session of its own, so a case's
//! row is the same alone, in a subset or in the whole census.

use rts_core_engine::engine;
use rts_core_engine::weapon_audit::{self, Case, Paths};

pub use weapon_audit::MAP;
/// The repo's test vault holding the audit map, read from the working
/// directory (the engine checkout) like the audit scripts.
pub const VAULT: &str = "tests/vault";
/// The camera to start watching the centred case at (the audit map's centre).
const AUDIT_CAMERA: &str = "512,512,150";

fn paths<'a>(init: &'a str, game: &'a str) -> Paths<'a> {
    Paths { init, game, vault: std::path::Path::new(VAULT) }
}

fn bail(msg: &str) -> std::process::ExitCode {
    eprintln!("{msg}");
    std::process::ExitCode::FAILURE
}

fn framework_failure(head: &str, failures: &[String]) -> std::process::ExitCode {
    eprintln!("{head}: the audit framework failed:");
    for f in failures {
        eprintln!("  {f}");
    }
    std::process::ExitCode::FAILURE
}

/// `WEAPON_AUDIT_IDS` (`a,b,c`; `n@b` a bearing sub-case): the census subset
/// to run, None = all.
fn selected_ids() -> Option<Vec<String>> {
    let v = std::env::var("WEAPON_AUDIT_IDS").ok()?;
    let ids: Vec<String> = v.split([',', ' ']).filter(|s| !s.is_empty()).map(Into::into).collect();
    (!ids.is_empty()).then_some(ids)
}

/// `--audit-table`: the whole census (or `WEAPON_AUDIT_IDS`' subset) as one
/// aligned table to stdout. Outcomes are data; only framework breakage is
/// nonzero.
pub fn run_table(init: &str, game: &str) -> std::process::ExitCode {
    let started = std::time::Instant::now();
    let threads = weapon_audit::threads();
    let bearings = weapon_audit::bearings();
    let census = match weapon_audit::census(paths(init, game), selected_ids().as_deref(), threads, &bearings) {
        Ok(c) => c,
        Err(e) => return bail(&format!("--audit-table: {e}")),
    };
    let wall = started.elapsed().as_secs_f64();
    if !census.unfinished.is_empty() {
        return bail(&format!("--audit-table: the audit did not finish: cases {:?} unresolved by their caps", census.unfinished));
    }
    if !census.failures.is_empty() {
        return framework_failure("--audit-table", &census.failures);
    }
    if census.cases.is_empty() || census.cases.len() != census.rows.len() {
        return bail(&format!("--audit-table: {} rows for {} cases", census.rows.len(), census.cases.len()));
    }
    let width = audit_table_width(terminal_columns());
    println!("{}", table(&census.cases, &census.rows, width));
    let mut by: std::collections::BTreeMap<&str, u32> = Default::default();
    for o in census.rows.iter().filter_map(|r| r.split('|').nth(4)) {
        *by.entry(o).or_default() += 1;
    }
    let totals = by.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ");
    println!("{}", clip(&format!("outcomes: {totals}"), width));
    let mut tokens: std::collections::BTreeMap<&str, std::collections::BTreeMap<&str, u32>> = Default::default();
    for (r, c) in census.rows.iter().zip(&census.cases) {
        let o = r.split('|').nth(4).unwrap_or("");
        if !c.reason_key.is_empty() && has_reason(o) {
            *tokens.entry(o).or_default().entry(c.reason_key.as_str()).or_default() += 1;
        }
    }
    for (o, ks) in &tokens {
        let mut counted: Vec<(&str, u32)> = ks.iter().map(|(k, n)| (*k, *n)).collect();
        counted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let list = counted.iter().map(|(k, n)| format!("{k} ({n})")).collect::<Vec<_>>().join(", ");
        println!("reasons: {}", clip(&format!("{o}: {list}"), width - 9));
    }
    for l in bearing_summary(&census.cases, &census.rows, &bearings) {
        println!("{}", clip(&l, width));
    }
    println!(
        "{} cases, {} sessions on {threads} threads (longest {} ticks), {wall:.1}s wall",
        census.cases.len(),
        census.sessions,
        census.max_ticks
    );
    std::process::ExitCode::SUCCESS
}

/// `--audit-test N` headless: the one case to its verdict, then the spot
/// and the census-format row on stdout (the census runs this very session
/// for the case). A bad id is the script's own discovery (its failure names
/// the count); clear message, nonzero.
pub fn run_case(n: &str, init: &str, game: &str) -> std::process::ExitCode {
    let started = std::time::Instant::now();
    let session = weapon_audit::script(&format!("case{}", weapon_audit::key_tag(n)), Some(&weapon_audit::case_prelude(n)))
        .and_then(|script| weapon_audit::run_session(paths(init, game), &script));
    let outcome = match session {
        Ok(ok) => ok,
        Err(e) => return bail(&format!("--audit-test: {e}")),
    };
    let wall = started.elapsed().as_secs_f64();
    if let Some(f) = outcome.failures.iter().find(|f| f.contains("case selection matched nothing")) {
        let count = lazy_regex::regex!(r"case count is (\d+)").captures(f).map(|c| c[1].to_owned());
        let no_bearings = lazy_regex::regex!(r"takes no bearings: ([^;]+)").captures(f).map(|c| c[1].to_owned());
        match (count, no_bearings) {
            (_, Some(why)) => eprintln!("case {n} does not exist: that weapon takes no bearings ({why})"),
            (Some(count), None) => eprintln!("case {n} does not exist; case count is {count}; run --audit-table for the list"),
            (None, None) => eprintln!("case {n} matched no case; run --audit-table for the list ({f})"),
        }
        return std::process::ExitCode::FAILURE;
    }
    if !outcome.failures.is_empty() {
        return framework_failure("--audit-test", &outcome.failures);
    }
    if !outcome.done {
        return bail(&format!(
            "--audit-test: the audit did not finish ({} rows for {} cases by the cap)",
            outcome.rows.len(),
            outcome.cases.len()
        ));
    }
    let Some(c) = outcome.cases.first() else {
        return bail("--audit-test: done but no case table");
    };
    assert!(
        c.key == n || (c.bearing == 0 && c.id.to_string() == n.split('@').next().unwrap_or("")),
        "the selected case {} is not the requested one ({n})",
        c.key
    );
    let Some(row) = outcome.rows.first() else {
        return bail(&format!("--audit-test: done but no row for case {n}"));
    };
    let at = if c.x == 0.0 && c.z == 0.0 {
        " (nothing placed; the case is non-attack, unsupported or the map has no spot)".to_owned()
    } else {
        format!(" centred at {:.0}, {:.0}", c.x, c.z)
    };
    println!(
        "case {n}: {}[{}] vs {}{} (budget {}, {} ticks, {wall:.1}s)",
        c.attacker, c.weapon, c.target, at, c.budget, outcome.ticks
    );
    println!("{row}");
    let o = row.split('|').nth(4).unwrap_or("");
    if has_reason(o) && !c.reason.is_empty() {
        println!("{o}: {}", c.reason);
    }
    if !c.aim.is_empty() {
        println!("{}", c.aim);
    }
    std::process::ExitCode::SUCCESS
}

/// `--audit-test N` windowed: the session a person watches (the case in
/// real time), after printing how to watch it. The exact spot is the
/// script's early note, reaching this console (and the log) seconds in.
/// `ticks` (`--ticks`) ends the game, closing the window, after that many
/// ticks; `None` runs until the window closes.
pub fn case_session(n: &str, ticks: Option<u64>) -> Result<engine::session::SessionConfig, String> {
    let script = weapon_audit::script(&format!("watch{}", weapon_audit::key_tag(n)), Some(&weapon_audit::case_prelude(n)))?;
    tracing::info!(
        "weapon audit case {n} runs alone on {MAP}, its engagement centred on the map's centre. Seconds \
after launch this console and the log show the exact spot ('audit: standalone: case {n} centred at \
x, z'); start zoomed there with --camera X,Z,DISTANCE (e.g. --camera {AUDIT_CAMERA}). The game runs {}.",
        match ticks {
            Some(t) => format!("{t} ticks, then the window closes"),
            None => "until the window closes".to_string(),
        }
    );
    Ok(engine::session::SessionConfig { ticks, human_army: Some(1), ..weapon_audit::session_config(script) })
}

/// The table's column budget in terminal columns: a TTY's `ioctl(TIOCGWINSZ)`
/// when stdout is a terminal (Unix), else an exported `COLUMNS`, else 100 —
/// clamped to the layout's working range (below its fixed-minimum sum the
/// 6-column table can no longer keep one case to one line).
fn terminal_columns() -> usize {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            return ws.ws_col as usize;
        }
    }
    std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok()).unwrap_or(100)
}

fn audit_table_width(cols: usize) -> usize {
    cols.clamp(80, 500)
}

/// The census table: one line per row, sized to `cols` columns so a row can
/// never wrap — spare width goes to attacker (cap 56), then target (40),
/// then outcome (30); over budget, those shed in reverse to their floors.
/// Attacker/target humanize from the case table's blueprint names and the
/// parenthesised bp id always survives truncation; non-attack, unsupported and
/// map-nosupport rows carry the script's short `reasonKey` token (the full
/// sentences stay in the case table and the log; the legend tallies tokens).
fn table(cases: &[Case], rows: &[String], cols: usize) -> String {
    const SEP: usize = 2;
    const ATT_CAP: usize = 56;
    const TGT_CAP: usize = 40;
    const OUT_CAP: usize = 30;
    const ATT_MIN: usize = 30;
    const TGT_MIN: usize = 22;
    const OUT_MIN: usize = 22;
    const ATT_FLOOR: usize = 24;
    const TGT_FLOOR: usize = 12;
    const OUT_FLOOR: usize = 13;

    let hdr: [String; 6] = ["id", "attacker", "target", "outcome", "fired", "damage"].map(Into::into);
    let right = [true, false, false, false, true, true];

    let mut id_w = 4usize;
    let (mut att_w, mut tgt_w, mut out_w) = (0usize, 0usize, 0usize);
    let mut fired_w = 4usize;
    let mut dmg_w = 6usize;
    for (r, c) in rows.iter().zip(cases) {
        let f: Vec<&str> = r.split('|').collect();
        id_w = id_w.max(f[0].chars().count());
        fired_w = fired_w.max(f[5].chars().count()).max(hdr[4].len());
        dmg_w = dmg_w.max(f[6].chars().count());
        att_w = att_w.max(attacker_cell(c, usize::MAX).chars().count());
        tgt_w = tgt_w.max(target_cell(c, usize::MAX).chars().count());
        out_w = out_w.max(outcome_text(f[4], c, usize::MAX).chars().count());
    }
    fired_w = fired_w.max(hdr[4].len());
    dmg_w = dmg_w.max(hdr[5].len());
    att_w = ATT_CAP.min(ATT_MIN.max(att_w));
    tgt_w = TGT_CAP.min(TGT_MIN.max(tgt_w));
    out_w = OUT_CAP.min(OUT_MIN.max(out_w));

    let budget = cols.saturating_sub(id_w + fired_w + dmg_w + SEP * 5);
    let att_want = ATT_CAP.min(att_w);
    let tgt_want = TGT_CAP.min(tgt_w);
    let out_want = OUT_CAP.min(out_w);
    let mut att_w = ATT_MIN;
    let mut tgt_w = TGT_MIN;
    let mut out_w = OUT_MIN;
    if att_w + tgt_w + out_w > budget {
        // Under the fixed minimums (a narrow terminal): shed in reverse
        // priority toward the floors — outcome, then target, then attacker.
        let mut over = att_w + tgt_w + out_w - budget;
        let floors = [(&mut out_w, OUT_FLOOR), (&mut tgt_w, TGT_FLOOR), (&mut att_w, ATT_FLOOR)];
        for (cell, floor) in floors {
            let take = cell.saturating_sub(floor).min(over);
            *cell -= take;
            over -= take;
        }
    } else {
        // Spare width beyond the minimums: attacker first (to its 56 cap),
        // then target (40), then outcome (30), each to content or cap.
        let mut spare = budget.saturating_sub(att_w + tgt_w + out_w);
        let grows = [(&mut att_w, att_want), (&mut tgt_w, tgt_want), (&mut out_w, out_want)];
        for (cell, want) in grows {
            let take = want.saturating_sub(*cell).min(spare);
            *cell += take;
            spare -= take;
        }
    }

    let widths = [id_w, att_w, tgt_w, out_w, fired_w, dmg_w];
    let mut out = line(&hdr, &widths, &right);
    for (r, c) in rows.iter().zip(cases) {
        let f: Vec<&str> = r.split('|').collect();
        let mut cells: [String; 6] = [
            f[0].to_owned(),
            attacker_cell(c, widths[1]),
            target_cell(c, widths[2]),
            outcome_text(f[4], c, widths[3]),
            f[5].to_owned(),
            f[6].to_owned(),
        ];
        let mut row_line = line(&cells, &widths, &right);
        if row_line.chars().count() > cols {
            let flood = row_line.chars().count() - cols;
            cells[3] = clip(cells[3].trim_end(), cells[3].chars().count().saturating_sub(flood));
            row_line = line(&cells, &widths, &right);
        }
        assert!(
            row_line.chars().count() <= cols,
            "--audit-table: row for case {} does not fit the {}-column budget",
            cells[0],
            cols
        );
        out.push('\n');
        out.push_str(&row_line);
    }
    out
}

/// The bearing lines under the table: per bearing, how many of the weapons
/// that take bearings pass there (bearing 0 = their canonical rows), then
/// one line per such weapon that misses at some bearing: its outcome at
/// each (the AUDIT lines' `aim` token has the miss distances;
/// `dev/audit/bearing_report.py` tabulates them).
fn bearing_summary(cases: &[Case], rows: &[String], set: &str) -> Vec<String> {
    use std::collections::BTreeMap;
    // id -> its rows (bearing, outcome), in census order (clockwise)
    let mut by_id: BTreeMap<u32, Vec<(i32, &str)>> = BTreeMap::new();
    for (c, r) in cases.iter().zip(rows) {
        by_id.entry(c.id).or_default().push((c.bearing, r.split('|').nth(4).unwrap_or("")));
    }
    let names: BTreeMap<u32, &Case> = cases.iter().filter(|c| c.bearing == 0).map(|c| (c.id, c)).collect();
    let mut tally: BTreeMap<i32, (u32, u32)> = BTreeMap::new();
    let mut misses = Vec::new();
    for (id, rs) in by_id.iter().filter(|(_, rs)| rs.iter().any(|(b, _)| *b != 0)) {
        for &(b, o) in rs {
            let t = tally.entry(b.rem_euclid(360)).or_default();
            t.1 += 1;
            if o == "pass" {
                t.0 += 1;
            }
        }
        if rs.iter().any(|(_, o)| *o != "pass") {
            let who = names.get(id).map_or_else(String::new, |c| format!(" {} w{}", c.attacker, c.weapon));
            let cells = rs.iter().map(|(b, o)| format!("{b} {o}")).collect::<Vec<_>>().join(", ");
            misses.push(format!("  {id}{who}: {cells}"));
        }
    }
    if tally.is_empty() {
        return Vec::new();
    }
    let weapons = by_id.values().filter(|rs| rs.iter().any(|(b, _)| *b != 0)).count();
    let per = tally
        .iter()
        .map(|(b, (p, n))| format!("{} {p}/{n}", if *b > 180 { b - 360 } else { *b }))
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = vec![format!("bearings ({set}, {weapons} weapons): pass {per}")];
    if !misses.is_empty() {
        out.push(format!("bearing misses ({} weapons, outcome per bearing):", misses.len()));
        out.extend(misses);
    }
    out
}

/// Outcomes that carry the script's reason (a never-engaged case): the
/// cell shows its token and the legend tallies it.
fn has_reason(o: &str) -> bool {
    matches!(o, "non-attack" | "unsupported" | "map-nosupport")
}

/// The outcome cell: outcome word plus the reason's `reasonKey` token when
/// the script set one, the full sentence as the fallback; the whole cell
/// clipped to `w` (the parenthetical degrades first, the word stays).
fn outcome_text(o: &str, c: &Case, w: usize) -> String {
    let clipped = has_reason(o) && !c.reason.is_empty();
    if !clipped {
        return clip(o, w);
    }
    let detail = if c.reason_key.is_empty() { c.reason.as_str() } else { c.reason_key.as_str() };
    let room = w.saturating_sub(o.chars().count() + 3);
    if room > 1 && room >= detail.chars().count() {
        format!("{o} ({detail})")
    } else if room > 1 {
        format!("{o} ({})", clip(detail, room))
    } else {
        clip(o, w)
    }
}

/// `head tail` within `cap`: the head is cut (suffix '...') to the room left
/// over, the tail is kept whole; a head that doesn't fit at all renders as
/// its own elision in front of a clipped tail.
fn two_part(head: &str, tail: &str, cap: usize) -> String {
    let room = cap.saturating_sub(tail.chars().count());
    if room == 0 {
        return format!("...{}", clip(tail, cap.saturating_sub(3)));
    }
    format!("{}{tail}", clip(head, room))
}

/// The attacker cell hides its weapon name before it lets the `(bp) wN`
/// tail (the id) elide, and falls back to the bare `bp[w]` form when even
/// that cannot stay whole.
fn attacker_cell(c: &Case, cap: usize) -> String {
    if c.att_name.is_empty() {
        return clip(&format!("{}[{}]", c.attacker, c.weapon), cap);
    }
    let id = format!(" ({}) w{}", c.attacker, c.weapon);
    let name_room = cap.saturating_sub(id.chars().count() + 1);
    if name_room < 5 {
        return two_part(&c.att_name, &id, cap).trim_end().into();
    }
    let name = clip(&c.weapon_name, name_room);
    two_part(&c.att_name, &format!("{id} {name}"), cap).trim_end().into()
}

/// The target cell: `name (bp)`, shrinking to the bare bp id (never a cut
/// id) when the column cannot hold the parenthesised form.
fn target_cell(c: &Case, cap: usize) -> String {
    if c.target == "-" {
        return "-".into();
    }
    if c.tgt_name.is_empty() {
        return clip(&c.target, cap);
    }
    let id = format!(" ({})", c.target);
    if id.chars().count() + 4 > cap {
        return clip(&c.target, cap);
    }
    two_part(&c.tgt_name, &id, cap).trim_end().into()
}

/// Overlong cells shrink to `cap` chars: the head kept with a '...' tail.
/// ASCII dots (not '…') so a row's char count is also its byte count — the
/// one-line-per-row guarantee holds for byte-counting consumers (grep).
fn clip(s: &str, cap: usize) -> String {
    let n = s.chars().count();
    if n <= cap {
        s.into()
    } else if cap <= 3 {
        s.chars().take(cap).collect()
    } else {
        format!("{}...", s.chars().take(cap.saturating_sub(3)).collect::<String>())
    }
}

fn line(cells: &[String; 6], widths: &[usize; 6], right: &[bool; 6]) -> String {
    (0..6)
        .map(|i| {
            if right[i] {
                format!("{:>1$}", cells[i], widths[i])
            } else {
                format!("{:<1$}", cells[i], widths[i])
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Census-shaped snapshot cases: the widest attacker names in the real
    /// run (ids 565/566, 22-char blueprint), every reason token class, a
    /// keyless reason, and plain engaged rows.
    fn cases() -> (Vec<Case>, Vec<String>) {
        let case = |id: u32, att: &str, w, tgt: &str, att_n: &str, w_n: &str, tgt_n: &str, r: &str, k: &str| {
            let attacker = att.to_owned();
            let att_name = att_n.to_owned();
            let weapon_name = w_n.to_owned();
            let tgt_name = tgt_n.to_owned();
            let reason = r.to_owned();
            let reason_key = k.to_owned();
            let target = tgt.to_owned();
            let (weapon, budget) = (w, 400);
            let key = id.to_string();
            Case {
                id, key, bearing: 0, aim: String::new(), attacker, weapon, target, x: 0.0, z: 0.0, budget,
                att_name, weapon_name, tgt_name, reason, reason_key,
            }
        };
        let rows = [
            "1|daa0206|1|-|map-nosupport|0|0",
            "3|dal0310|1|ueb1101|fired-no-hit|77|0",
            "46|uaa0304|2|uab1101|pass|1|3000",
            "246|uel0401|9|-|unsupported|0|0",
            "510|ueb2101|1|-|unsupported|0|0",
            "565|xsl0301_advancedcombat|4|-|non-attack|0|0",
            "566|xsl0301_advancedcombat|5|-|unsupported|0|0",
            "700|xal0305|1|-|unsupported|0|0",
        ];
        let cases = vec![
            case(1, "daa0206", 1, "-", "Aeon T2 Guided Missile: Mercy", "Kamikaze", "-",
                "no fitting cell (start medium dry, far end dry)", "no-cell"),
            case(3, "dal0310", 1, "ueb1101", "Aeon T3 Shield Disruptor", "Shield Disruptor Cannon",
                "UEF T1 Power Generator", "", ""),
            case(46, "uaa0304", 2, "uab1101", "Aeon T3 Strategic Bomber: Shockwave", "Quark Bomb",
                "Aeon T1 Power Generator", "", ""),
            case(246, "uel0401", 9, "-", "UEF Experimental Mobile Factory", "Angler Torpedo", "-",
                "no reachable layer (live caps empty)", "no-caps"),
            case(510, "ueb2101", 1, "-", "UEF T2 Point Defense", "FD-1 Anti-Missile Battery", "-",
                "anti-projectile (anti-missile): engages projectiles, not units", "anti-proj:missile"),
            case(565, "xsl0301_advancedcombat", 4, "-", "Seraphim SACU (Advanced Combat Suit)", "Death Nuke", "-",
                "death weapon", "death"),
            case(566, "xsl0301_advancedcombat", 5, "-", "Seraphim SACU (Advanced Combat Suit)",
                "Laanse Tactical Missile Launcher", "-", "silo weapon: needs built-up ammo", "silo"),
            case(700, "xal0305", 1, "-", "Aeon T2 Sniper Tower", "Sniper Rifle", "-",
                "no fighter target in the catalog for xal0305", ""),
        ];
        (cases, rows.map(Into::into).to_vec())
    }

    #[test]
    fn one_line_per_row_at_common_widths() {
        let (cases, rows) = cases();
        for cols in [80, 100, 120, 200] {
            let t = table(&cases, &rows, cols);
            assert_eq!(t.lines().count(), rows.len() + 1, "{cols} cols: header plus one line per row");
            for line in t.lines() {
                assert!(line.chars().count() <= cols, "{cols} cols: {} chars: {line}", line.chars().count());
            }
            for id in ["xsl0301_advancedcombat", "xal0305"] {
                let line = t.lines().find(|l| l.contains(id));
                assert!(
                    line.is_some_and(|l| l.contains(id)),
                    "{cols} cols: the wide-name case's blueprint id does not survive the row: {line:?}"
                );
            }
        }
        let t = table(&cases, &rows, 100);
        let l3 = t.lines().nth(2).unwrap();
        assert!(l3.contains("ueb1101") && l3.contains("(dal0310)"), "100 cols: {l3}");
        let l46 = t.lines().nth(3).unwrap();
        assert!(l46.contains("Quark Bomb") && l46.contains("(uaa0304)"), "100 cols: {l46}");
        assert!(t.lines().nth(4).unwrap().contains("Angler Torpedo"), "100 cols: {}", t.lines().nth(4).unwrap());
    }

    #[test]
    fn case_keys_and_the_bearing_summary() {
        for k in ["46", "531@180", "238@-90"] {
            assert!(weapon_audit::is_case_key(k), "{k}");
        }
        for k in ["", "46@", "@90", "46@+90", "46.3", "46@9x", "a1"] {
            assert!(!weapon_audit::is_case_key(k), "{k}");
        }
        let (mut cases, mut rows) = cases();
        let sub = |c: &Case, b: i32| Case { key: format!("{}@{b}", c.id), bearing: b, ..c.clone() };
        let c46 = cases.iter().find(|c| c.id == 46).unwrap().clone();
        let at = cases.iter().position(|c| c.id == 46).unwrap();
        cases.insert(at + 1, sub(&c46, 180));
        rows.insert(at + 1, "46@180|uaa0304|2|uab1101|held-no-fire|0|0".into());
        let lines = bearing_summary(&cases, &rows, "gate");
        assert_eq!(lines[0], "bearings (gate, 1 weapons): pass 0 1/1, 180 0/1");
        assert_eq!(lines[2], "  46 uaa0304 w2: 0 pass, 180 held-no-fire");
    }

    #[test]
    fn width_budget_is_the_clamped_detected_width() {
        assert_eq!(audit_table_width(100), 100);
        assert_eq!(audit_table_width(60), 80);
        assert_eq!(audit_table_width(0), 80);
        assert_eq!(audit_table_width(9999), 500);
    }
}
