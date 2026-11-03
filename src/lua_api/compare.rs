//! Compares a VM's registered API with FAF's annotations of moho's API
//! (`/engine/{Core,Sim,User}.lua` and `/engine/{Core,Sim,User}/*.lua` in the
//! FAF checkout). FAF's files are community documentation, not ground
//! truth: this is a report to read (what we lack, what we have that FAF
//! doesn't describe, likely typos, arity differences), never a gate.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

use super::emit::signature;
use super::registry::{ApiRegistry, Scope, Status};
use super::ty::LuaTy;

/// A function FAF declares: parameter names (`...` for varargs).
#[derive(Clone, Debug, Default)]
pub struct FafFn {
    pub params: Vec<String>,
    pub file: String,
    pub line: usize,
}

/// FAF's declarations for one VM: globals, table functions, class methods
/// (by moho class name), and class bases.
#[derive(Debug, Default)]
pub struct FafApi {
    pub fns: BTreeMap<(Scope, String), FafFn>,
    pub bases: BTreeMap<String, Vec<String>>,
}

/// Which of FAF's `/engine` files describe a VM: `Core` and `Library` (the
/// standard library as moho changed it: `LOG`, `_ALERT`, ...) plus `Sim` or
/// `User`.
pub fn read_faf(faf: &Path, vm_dir: &str) -> std::io::Result<FafApi> {
    let engine = faf.join("engine");
    let mut files = vec![engine.join("Core.lua"), engine.join("Library.lua"), engine.join(format!("{vm_dir}.lua"))];
    for dir in ["Core", vm_dir] {
        let mut entries: Vec<_> = std::fs::read_dir(engine.join(dir))?.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "lua")).collect();
        entries.sort();
        files.extend(entries);
    }
    let mut api = FafApi::default();
    for path in files {
        let text = std::fs::read_to_string(&path)?;
        let rel = path.strip_prefix(faf).unwrap_or(&path).display().to_string();
        let untagged = UNTAGGED.iter().find(|(p, _)| p.eq_ignore_ascii_case(&format!("/{}", rel.replace('\\', "/")))).map(|(_, c)| *c);
        parse_file(&text, &rel, untagged, &mut api);
    }
    Ok(api)
}

/// FAF's `/engine` files whose class isn't tagged `moho.<class>`: path (as
/// the VFS has it) and the moho class their first `---@class` describes.
pub const UNTAGGED: &[(&str, &str)] = &[("/engine/user/cameraimpl.lua", "CameraImpl_methods"), ("/engine/user/userunit.lua", "userunit_methods")];

/// Functions we register that FAF's annotations don't declare, whose origin
/// is checked: (scope label, name, origin). The report lists them
/// apart, so an unexplained one stands out. Each one's doc says more.
pub const ORIGINS: &[(&str, &str, &str)] = &[
    ("`Replay` table", "AddEvent", "ours: the replay timeline"),
    ("`Replay` table", "GetEvents", "ours: the replay timeline"),
    ("`Replay` table", "GetLength", "ours: the replay timeline"),
    ("`Replay` table", "GetSeekTarget", "ours: the replay timeline"),
    ("`Replay` table", "IsSeeking", "ours: the replay timeline"),
    ("`Replay` table", "Seek", "ours: the replay timeline"),
    ("`debug` table", "getinfo", "Lua 5.0's, which FAF's code calls (Luau has `debug.info`)"),
    ("globals", "dofile", "Lua's standard library"),
    ("globals", "require", "Lua's standard library (disabled: scripts load through import)"),
    ("globals", "repr", "FAF's (`/lua/system/repr.lua`), provided before it loads"),
    ("globals", "EntityCategoryFilterOut", "moho's, user VM only (MohoEngine.dll); ours in both"),
    ("globals", "EntityCategoryCount", "moho's, sim (SupremeCommander.exe); ours in both"),
    ("globals", "GetUnitBlueprintByName", "moho's, sim only (MohoEngine.dll); ours in both"),
    ("`moho.entity_methods`", "HideBone", "moho's on units only (MohoEngine.dll); ours on any entity"),
    ("`moho.entity_methods`", "ShowBone", "moho's on units only (MohoEngine.dll); ours on any entity"),
    ("`moho.entity_methods`", "RevertCollisionShape", "moho's on units only (MohoEngine.dll); ours on any entity"),
    ("`moho.IEffect`", "BeenDestroyed", "moho's on entities, weapons, slide manipulators; ours wider"),
    ("`moho.manipulator_methods`", "BeenDestroyed", "moho's on the slide manipulator only; ours on every one"),
    ("`moho.unit_methods`", "GetCreator", "moho's on `UserUnit` only (SupremeCommander.exe); ours on the sim's too"),
    ("`moho.unit_methods`", "IsAutoSurfaceMode", "moho's on `UserUnit` only; ours carries the toggle to the sim"),
    ("`moho.unit_methods`", "SetAutoSurfaceMode", "moho's as a user global only; ours carries the toggle to the sim"),
    ("`moho.userunit_methods`", "AddSelectionSet", "moho's (MohoEngine.dll, `UserUnit`)"),
    ("`moho.text_methods`", "SetVertical", "ours: an engine extension (the ping grid's labels)"),
];

/// FAF annotations that put a function in the wrong place, as the
/// binary's registration settles it: (FAF's scope label, name, moho's class).
/// They're listed apart rather than as missing.
pub const MISFILED: &[(&str, &str, &str)] = &[("globals", "SinkAway", "Entity"), ("`CMauiFrame` table", "ClearChildren", "CMauiControl")];

fn parse_file(text: &str, file: &str, untagged: Option<&str>, api: &mut FafApi) {
    // `---@class moho.X : moho.Y` then `local Var = {}`: methods `Var:Name`.
    let mut pending_class: Option<String> = None;
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let l = line.trim();
        if let Some(class) = untagged.filter(|_| vars.is_empty() && pending_class.is_none() && l.starts_with("---@class ")) {
            api.bases.insert(class.to_string(), Vec::new());
            pending_class = Some(class.to_string());
            continue;
        }
        if let Some(rest) = l.strip_prefix("---@class moho.") {
            let (name, bases) = match rest.split_once(':') {
                Some((n, b)) => (n.trim(), b),
                None => (rest.trim(), ""),
            };
            // FAF sometimes follows the name with prose (`moho.CDamage This
            // particular class is not made approachable...`).
            let name = name.split_whitespace().next().unwrap_or("");
            let bases: Vec<String> = bases.split(',').filter_map(|b| b.trim().strip_prefix("moho.")).map(|b| b.trim().to_string()).collect();
            api.bases.insert(name.to_string(), bases);
            pending_class = Some(name.to_string());
            continue;
        }
        if let Some(rest) = l.strip_prefix("local ") {
            if let (Some(class), Some((var, value))) = (&pending_class, rest.split_once('='))
                && value.trim().starts_with('{') {
                vars.insert(var.trim().to_string(), class.clone());
                pending_class = None;
            }
            continue;
        }
        let Some(rest) = l.strip_prefix("function ") else { continue };
        let Some((head, tail)) = rest.split_once('(') else { continue };
        let params: Vec<String> = tail.split(')').next().unwrap_or("").split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        let (scope, name) = if let Some((var, m)) = head.split_once(':') {
            match vars.get(var.trim()) {
                Some(class) => (Scope::Class(class.clone().into()), m.trim().to_string()),
                None => continue,
            }
        } else if let Some((t, f)) = head.split_once('.') {
            (Scope::Table(t.trim().to_string().into()), f.trim().to_string())
        } else {
            (Scope::Global, head.trim().to_string())
        };
        // Metamethods (`EntityCategory.__add`) are userdata behaviour, not
        // functions scripts call by name.
        if name.starts_with("__") {
            continue;
        }
        api.fns.insert((scope, name), FafFn { params, file: file.to_string(), line: i + 1 });
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

fn scope_label(s: &Scope) -> String {
    match s {
        Scope::Global => "globals".into(),
        Scope::Table(t) => format!("`{t}` table"),
        Scope::Class(c) => format!("`moho.{c}`"),
    }
}

fn qualified(s: &Scope, name: &str) -> String {
    match s {
        Scope::Global => name.to_string(),
        Scope::Table(t) => format!("{t}.{name}"),
        Scope::Class(c) => format!("{c}:{name}"),
    }
}

/// Classes `c` inherits from (itself first), by our registry's bases and FAF's.
fn lineage(c: &str, reg: &ApiRegistry, faf: &FafApi) -> Vec<String> {
    let mut out = vec![c.to_string()];
    let mut i = 0;
    while i < out.len() {
        let cur = out[i].clone();
        let ours = reg.class(&cur).map(|e| e.bases.iter().map(|b| b.to_string()).collect::<Vec<_>>()).unwrap_or_default();
        for b in ours.into_iter().chain(faf.bases.get(&cur).cloned().unwrap_or_default()) {
            if !out.contains(&b) {
                out.push(b);
            }
        }
        i += 1;
    }
    out
}

/// Counts for one VM's comparison.
#[derive(Debug, Default, Clone, Copy)]
pub struct Summary {
    pub faf_declared: usize,
    pub registered: usize,
    pub implemented: usize,
    pub stubs: usize,
    pub missing: usize,
    pub not_in_faf: usize,
    pub likely_typos: usize,
    pub arity: usize,
}

/// The comparison report for one VM, as markdown, and its counts.
pub fn report(reg: &ApiRegistry, faf: &FafApi, vm: &str) -> (String, Summary) {
    let ours: BTreeMap<(Scope, String), &super::registry::FnEntry> = reg.functions().into_iter().map(|f| ((f.scope.clone(), f.name.to_string()), f)).collect();
    let has = |scope: &Scope, name: &str| -> bool {
        match scope {
            Scope::Class(c) => lineage(c, reg, faf).iter().any(|k| ours.contains_key(&(Scope::Class(k.clone().into()), name.to_string()))),
            s => ours.contains_key(&(s.clone(), name.to_string())),
        }
    };
    let faf_has = |scope: &Scope, name: &str| -> bool {
        match scope {
            Scope::Class(c) => lineage(c, reg, faf).iter().any(|k| faf.fns.contains_key(&(Scope::Class(k.clone().into()), name.to_string()))),
            s => faf.fns.contains_key(&(s.clone(), name.to_string())),
        }
    };
    let mut s = Summary { faf_declared: faf.fns.len(), registered: ours.len(), ..Default::default() };
    for f in ours.values() {
        match f.status {
            Status::Stub => s.stubs += 1,
            _ => s.implemented += 1,
        }
    }
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut misfiled = Vec::new();
    for (scope, name) in faf.fns.keys() {
        if let Some((label, _, class)) = MISFILED.iter().find(|(l, n, _)| *n == name && *l == scope_label(scope)) {
            misfiled.push(format!("`{name}`: FAF has it in {label}, moho registers it on `{class}`"));
            continue;
        }
        if !has(scope, name) {
            missing.entry(scope_label(scope)).or_default().push(name.clone());
            s.missing += 1;
        }
    }
    let mut extra: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut known = Vec::new();
    let mut typos = Vec::new();
    for ((scope, name), f) in &ours {
        if f.status == Status::Internal || name.starts_with("__") || faf_has(scope, name) {
            continue;
        }
        if let Some((_, _, origin)) = ORIGINS.iter().find(|(l, n, _)| n == name && *l == scope_label(scope)) {
            known.push(format!("`{}`: {origin}", qualified(scope, name)));
            continue;
        }
        s.not_in_faf += 1;
        extra.entry(scope_label(scope)).or_default().push(name.clone());
        let near: BTreeSet<&str> = faf
            .fns
            .keys()
            .filter(|(fs, _)| fs == scope)
            .map(|(_, n)| n.as_str())
            .filter(|n| !has(scope, n) && (edit_distance(n, name) <= 2 || n.eq_ignore_ascii_case(name)))
            .collect();
        if !near.is_empty() {
            typos.push(format!("`{}`: FAF has {}", qualified(scope, name), near.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")));
        }
    }
    s.likely_typos = typos.len();
    let mut arity = Vec::new();
    for ((scope, name), f) in &ours {
        let Some(ff) = faf.fns.get(&(scope.clone(), name.clone())) else { continue };
        if f.status == Status::Stub {
            continue;
        }
        let sig = signature(f);
        let ours_var = sig.params.last().is_some_and(|(_, t)| matches!(t, LuaTy::Variadic(_)));
        let faf_var = ff.params.last().is_some_and(|p| p == "...");
        if ours_var || faf_var {
            continue;
        }
        // Trailing optional parameters may be left out: a mismatch is a FAF
        // count outside [required, all].
        let required = sig.params.iter().rposition(|(_, t)| !matches!(t, LuaTy::Optional(_))).map_or(0, |i| i + 1);
        if ff.params.len() < required || ff.params.len() > sig.params.len() {
            s.arity += 1;
            arity.push(format!(
                "`{}`: FAF `({})` ({}:{}), ours `({})`",
                qualified(scope, name),
                ff.params.join(", "),
                ff.file,
                ff.line,
                sig.params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let mut out = String::new();
    let _ = writeln!(out, "## {vm} VM\n");
    let _ = writeln!(out, "| | count |\n|---|---|");
    let _ = writeln!(out, "| FAF declares | {} |", s.faf_declared);
    let _ = writeln!(out, "| we register | {} ({} implemented or partial, {} stubs) |", s.registered, s.implemented, s.stubs);
    let _ = writeln!(out, "| FAF declares, we don't register | {} |", s.missing);
    let _ = writeln!(out, "| we register, FAF doesn't declare | {} |", s.not_in_faf);
    let _ = writeln!(out, "| ...of which look like a typo of a FAF name | {} |", s.likely_typos);
    let _ = writeln!(out, "| parameter count differs from FAF | {} |\n", s.arity);
    if !typos.is_empty() {
        let _ = writeln!(out, "### Likely typos (ours vs FAF's spelling)\n");
        typos.iter().for_each(|t| { let _ = writeln!(out, "- {t}"); });
        let _ = writeln!(out);
    }
    if !arity.is_empty() {
        let _ = writeln!(out, "### Parameter count differs\n");
        arity.iter().for_each(|t| { let _ = writeln!(out, "- {t}"); });
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "### FAF declares, we don't register\n");
    for (scope, names) in &missing {
        let _ = writeln!(out, "- {scope} ({}): {}", names.len(), names.join(", "));
    }
    if !misfiled.is_empty() {
        let _ = writeln!(out, "\n### FAF files elsewhere than moho\n");
        misfiled.iter().for_each(|t| { let _ = writeln!(out, "- {t}"); });
    }
    let _ = writeln!(out, "\n### We register, FAF doesn't declare\n");
    for (scope, names) in &extra {
        let _ = writeln!(out, "- {scope} ({}): {}", names.len(), names.join(", "));
    }
    if !known.is_empty() {
        let _ = writeln!(out, "\n### We register, FAF doesn't declare: origin known (`compare::ORIGINS`)\n");
        known.iter().for_each(|t| { let _ = writeln!(out, "- {t}"); });
    }
    let _ = writeln!(out);
    (out, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(c: &str, name: &str) -> (Scope, String) {
        (Scope::Class(c.to_string().into()), name.to_string())
    }

    #[test]
    fn class_name_stops_at_prose() {
        let mut api = FafApi::default();
        parse_file("---@class moho.navigator_methods This particular class is not made approachable\nlocal CNavigator = {}\nfunction CNavigator:AtGoal()\nend\n", "f", None, &mut api);
        assert!(api.fns.contains_key(&class("navigator_methods", "AtGoal")));
    }

    #[test]
    fn untagged_file_maps_to_its_class() {
        let mut api = FafApi::default();
        parse_file("---@class UserUnit: InternalObject\nlocal UserUnit = {}\n---@class Other\nfunction UserUnit:IsDead()\nend\n", "f", Some("userunit_methods"), &mut api);
        assert_eq!(api.fns.keys().cloned().collect::<Vec<_>>(), vec![class("userunit_methods", "IsDead")]);
    }

    #[test]
    fn metamethods_are_skipped() {
        let mut api = FafApi::default();
        parse_file("function EntityCategory.__add(a, b)\nend\nfunction EntityCategory.Other()\nend\n", "f", None, &mut api);
        assert_eq!(api.fns.len(), 1);
    }
}
