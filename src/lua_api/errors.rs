//! Explains a native function's argument error with its registered
//! signature. mlua reports `bad argument #2: error converting Lua nil to
//! f64` without saying which function or parameter; the function's name is
//! in the error's traceback (`[C]: in function 'SetBuildRate'`), and the
//! registry knows its signature. Done when an error is logged, so a call
//! that succeeds costs nothing (decoding arguments ourselves to name them
//! up front measured 45 vs 25 ns a call).

use std::fmt;

use mlua::prelude::*;

use super::emit::{luau_ty, signature};
use super::registry::{ApiRegistry, Scope};
use super::ty::LuaTy;

/// `{e}` plus, for a native function's argument error, which function and
/// parameter it was. Use in place of the error in Lua error logs.
pub struct Explained<'a>(pub &'a Lua, pub &'a LuaError);

impl fmt::Display for Explained<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.1)?;
        if let Some(x) = explain(self.0, self.1) {
            write!(f, "\n  {x}")?;
        }
        Ok(())
    }
}

/// The innermost argument error: (position, cause, traceback of the native
/// call that raised it).
fn bad_argument<'e>(e: &'e LuaError, tb: Option<&'e str>) -> Option<(usize, &'e LuaError, Option<&'e str>)> {
    match e {
        LuaError::CallbackError { traceback, cause } => bad_argument(cause, Some(traceback)),
        LuaError::WithContext { cause, .. } => bad_argument(cause, tb),
        LuaError::BadArgument { pos, cause, .. } => Some((*pos, cause, tb)),
        _ => None,
    }
}

/// The native function a traceback starts in (`[C]: in function 'X'`).
fn native_name(traceback: &str) -> Option<&str> {
    let line = traceback.lines().find(|l| l.trim_start().starts_with("[C]:"))?;
    let start = line.find('\'')? + 1;
    let end = start + line[start..].find('\'')?;
    Some(&line[start..end])
}

/// "Unit:SetBuildRate(rate: number) — argument #2 `rate` should be number".
pub fn explain(lua: &Lua, err: &LuaError) -> Option<String> {
    let (pos, _cause, tb) = bad_argument(err, None)?;
    let name = native_name(tb?)?;
    let reg = lua.app_data_ref::<ApiRegistry>()?;
    // Registered functions are named `Class:Method`, `table.fn` or `fn`.
    let found: Vec<_> = if let Some((class, method)) = name.split_once(':') {
        reg.named(method).into_iter().filter(|f| matches!(&f.scope, Scope::Class(c) if c == class || super::class::luau_name(c) == Some(class))).collect()
    } else if let Some((table, f)) = name.split_once('.') {
        reg.named(f).into_iter().filter(|e| matches!(&e.scope, Scope::Table(t) if t == table)).collect()
    } else {
        reg.named(name).into_iter().filter(|f| f.scope == Scope::Global).collect()
    };
    if found.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    for f in found.iter().take(4) {
        let sig = signature(f);
        let (qualified, offset) = match &f.scope {
            Scope::Class(c) => (format!("{}:{}", super::class::luau_name(c).unwrap_or(c), f.name), 2),
            Scope::Table(t) => (format!("{t}.{}", f.name), 1),
            Scope::Global => (f.name.to_string(), 1),
        };
        let params = sig.params.iter().map(|(n, t)| format!("{n}: {}", luau_ty(t))).collect::<Vec<_>>().join(", ");
        let which = match pos.checked_sub(offset) {
            None => "argument #1 is `self` (call it with `:`)".to_string(),
            Some(i) => match sig.params.get(i).or_else(|| sig.params.last().filter(|(_, t)| matches!(t, LuaTy::Variadic(_)))) {
                Some((n, t)) => format!("argument #{pos} `{n}` should be {}", luau_ty(t)),
                None => format!("argument #{pos} is past its {} parameter(s)", sig.params.len()),
            },
        };
        lines.push(format!("{qualified}({params}): {which}"));
    }
    let more = if found.len() > 4 { format!(" (and {} more)", found.len() - 4) } else { String::new() };
    Some(if lines.len() == 1 { format!("in {}", lines[0]) } else { format!("in one of: {}{more}", lines.join("; ")) })
}
