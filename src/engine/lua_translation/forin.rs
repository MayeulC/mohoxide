//! Restores Lua 5.0 generic-`for` semantics over tables.
//!
//! Lua 5.0: `for k, v in t do` with a *table* `t` iterates it with `next`.
//! Luau's generalized iteration instead *calls* a table that has a `__call`
//! metamethod (or uses `__iter`) — FAF class tables have `__call`, so
//! `for k, v in class do` (class.lua) would construct instances forever.
//!
//! Every generic `for` with a single, non-call expression is rewritten to
//! `for k, v in __forin(t) do`; `__forin` (lua/shims/lua50_aliases.lua) returns
//! `next, t` for tables and passes iterator functions through. Returning the
//! builtin `next` keeps Luau's fast iteration path. Calls such as `pairs(t)` /
//! `ipairs(t)` are left alone. Same-line splice, so line numbers are kept.

use super::lexscan;
use full_moon::LuaVersion;
use full_moon::ast::{Expression, GenericFor};
use full_moon::visitors::Visitor;

const OPEN: &str = "__forin(";
const CLOSE: &str = ")";

pub fn rewrite(source: &str) -> String {
    // Cheap pre-filter: no generic for loops at all.
    if !source.contains(" in ") {
        return source.to_owned();
    }
    match token_spans(source) {
        Some(spans) => splice(source, spans),
        None => rewrite_ast(source),
    }
}

/// The spans to wrap, found from the tokens alone (no AST: a full parse of
/// every script was most of startup's translation time). `None` when a loop
/// header holds a function body (its own `do`s and statements), which the
/// full parse handles instead.
///
/// A generic `for` is wrapped unless its expression list is several
/// expressions or exactly one call chain (`pairs(t)`, `a.b:c(x)`,
/// `f "s"`), as [`Collector`] decides from the AST.
pub(super) fn token_spans(source: &str) -> Option<Vec<(usize, usize)>> {
    use lexscan::Tok;
    let toks = lexscan::tokens(source);
    let text = |i: usize| &source[toks[i].1..toks[i].2];
    let name = |i: usize, s: &str| toks[i].0 == Tok::Name && text(i) == s;
    let mut spans = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if !name(i, "for") {
            i += 1;
            continue;
        }
        // The name list, then `in` (a generic for) or `=` (numeric).
        let mut j = i + 1;
        while j < toks.len() && (toks[j].0 == Tok::Name || toks[j].0 == Tok::Punct(b',')) && !name(j, "in") {
            j += 1;
        }
        if j >= toks.len() || !name(j, "in") {
            i = j;
            continue;
        }
        let first = j + 1;
        // Its `do`: the first at bracket depth 0 (only a function body could
        // hold another).
        let mut depth = 0i32;
        let mut multiple = false;
        let mut k = first;
        loop {
            let (kind, ..) = *toks.get(k)?;
            match kind {
                Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
                Tok::Punct(b')' | b']' | b'}') => depth -= 1,
                Tok::Punct(b',') if depth == 0 => multiple = true,
                Tok::Name if text(k) == "function" => return None,
                Tok::Name if depth == 0 && text(k) == "do" => break,
                _ => {}
            }
            k += 1;
        }
        if k == first {
            return None;
        }
        if !multiple && !is_call_chain(&toks[first..k]) {
            spans.push((toks[j].2, toks[k].1));
        }
        i = k + 1;
    }
    Some(spans)
}

/// Whether `toks` (one balanced expression) is exactly a function call: a
/// name or parenthesised prefix, then field / index / method suffixes,
/// ending in call arguments.
fn is_call_chain(toks: &[(lexscan::Tok, usize, usize)]) -> bool {
    use lexscan::Tok;
    // Index just past the bracket group opening at `i`.
    let close = |mut i: usize| {
        let mut depth = 0;
        while i < toks.len() {
            match toks[i].0 {
                Tok::Punct(b'(' | b'[' | b'{') => depth += 1,
                Tok::Punct(b')' | b']' | b'}') => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        toks.len() + 1
    };
    let mut i = match toks.first() {
        Some((Tok::Name, ..)) => 1,
        Some((Tok::Punct(b'('), ..)) => close(0),
        _ => return false,
    };
    let mut call = false;
    while i < toks.len() {
        match toks[i].0 {
            Tok::Punct(b'.') if toks.get(i + 1).is_some_and(|t| t.0 == Tok::Name) => {
                i += 2;
                call = false;
            }
            Tok::Punct(b'[') => {
                i = close(i);
                call = false;
            }
            Tok::Punct(b':') if toks.get(i + 1).is_some_and(|t| t.0 == Tok::Name) => {
                i += 2;
                match toks.get(i).map(|t| t.0) {
                    Some(Tok::Punct(b'(' | b'{')) => i = close(i),
                    Some(Tok::Str) => i += 1,
                    _ => return false,
                }
                call = true;
            }
            Tok::Punct(b'(' | b'{') => {
                i = close(i);
                call = true;
            }
            Tok::Str => {
                i += 1;
                call = true;
            }
            _ => return false,
        }
    }
    // A keyword in the prefix's place (`not f()`, `nil`) comes out as not a
    // call: the operand after it isn't a suffix, or there are none.
    i == toks.len() && call
}

/// Wraps each span in `__forin( … )`, trimmed of surrounding whitespace.
pub(super) fn splice(source: &str, mut spans: Vec<(usize, usize)>) -> String {
    if spans.is_empty() {
        return source.to_owned();
    }
    spans.sort_unstable();
    let mut out = String::with_capacity(source.len() + spans.len() * (OPEN.len() + CLOSE.len()));
    let mut last = 0;
    for (start, end) in spans {
        let (start, end) = trim_span(source, start, end);
        out.push_str(&source[last..start]);
        out.push_str(OPEN);
        out.push_str(&source[start..end]);
        out.push_str(CLOSE);
        last = end;
    }
    out.push_str(&source[last..]);
    out
}

/// The rewrite from a full parse: loop headers the token scan leaves to it,
/// and the reference the token scan is tested against.
pub(super) fn rewrite_ast(source: &str) -> String {
    let version = LuaVersion::lua53().with_luau();
    let result = full_moon::parse_fallible(source, version);
    if !result.errors().is_empty() {
        tracing::warn!(
            "lua_translation: full_moon parse failed during for-in rewrite \
             ({} error(s)); leaving source unchanged: {}",
            result.errors().len(),
            result.errors().first().map(|e| e.to_string()).unwrap_or_default(),
        );
        return source.to_owned();
    }

    let mut collector = Collector::default();
    collector.visit_ast(result.ast());
    splice(source, collector.spans)
}

#[derive(Default)]
struct Collector {
    spans: Vec<(usize, usize)>,
}

impl Visitor for Collector {
    fn visit_generic_for(&mut self, node: &GenericFor) {
        let exprs = node.expressions();
        if exprs.len() != 1 {
            return;
        }
        let Some(expr) = exprs.iter().next() else { return };
        if matches!(expr, Expression::FunctionCall(_)) {
            return;
        }
        // Span between `in` and `do` (trimmed later) — more reliable than the
        // expression's own range, which can stop before a trailing `]`.
        let start = node.in_token().token().end_position().bytes();
        let end = node.do_token().token().start_position().bytes();
        self.spans.push((start, end));
    }
}

/// Shrinks `(start, end)` to exclude surrounding whitespace.
fn trim_span(src: &str, start: usize, end: usize) -> (usize, usize) {
    let text = &src[start..end];
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    (start + lead, end - trail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIM: &str = include_str!("../lua/shims/lua50_aliases.lua");

    fn run(src: &str) -> String {
        let lua = mlua::Lua::new();
        lua.load(SHIM).exec().unwrap();
        lua.load(rewrite(src)).eval().unwrap()
    }

    #[test]
    fn wraps_plain_table_iteration_only() {
        assert_eq!(
            rewrite("for k, v in t do end for k, v in pairs(t) do end for i, v in a.b[c] do end"),
            "for k, v in __forin(t) do end for k, v in pairs(t) do end for i, v in __forin(a.b[c]) do end"
        );
        assert_eq!(rewrite("for k, v in next, t do end"), "for k, v in next, t do end");
    }

    /// The token scan wraps exactly what the full parse does, over the
    /// expression shapes that decide it.
    #[test]
    fn token_scan_matches_full_parse() {
        for src in [
            "for k, v in t do end",
            "for k, v in pairs(t) do end",
            "for k, v in a.b[c].d do end",
            "for k, v in a.b:c(x) do end",
            "for k, v in a:b { 1 } do end",
            "for k, v in f 's' do end",
            "for k, v in f[[s]] do end",
            "for k, v in (t) do end",
            "for k, v in (f)() do end",
            "for k, v in f() or {} do end",
            "for k, v in a .. b do end",
            "for k, v in not f() do end",
            "for k, v in t[1] do end",
            "for k, v in f()[1] do end",
            "for k, v in next, t do end",
            "for k, v in t --[[ c ]] do end",
            "for k, v in t -- c\n do end",
            "for i = 1, 10 do for k in t do end end",
            "local s = 'for x in y do' for k in t do end",
            "for k in f(function() for j in t do end end) do end",
            "for k in {1, 2} do end",
            "for k in 1.5e+3 do end",
        ] {
            assert_eq!(rewrite(src), rewrite_ast(src), "{src}");
        }
    }

    #[test]
    fn callable_table_iterates_with_next() {
        let src = "local calls = 0
            local t = setmetatable({ a = 1 }, { __call = function() calls = calls + 1 end })
            local n = 0
            for k, v in t do n = n + v end
            return n .. ':' .. calls";
        assert_eq!(run(src), "1:0");
    }

    #[test]
    fn iterator_function_still_works() {
        let src = "local i = 0
            local function it() i = i + 1 if i <= 3 then return i end end
            local s = 0
            for x in it do s = s + x end
            return tostring(s)";
        assert_eq!(run(src), "6");
    }
}
