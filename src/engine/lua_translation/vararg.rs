//! Restores Lua 5.0's implicit `arg` table in vararg functions.
//!
//! In Lua 5.0 every `function(...)` gets a local `arg = {n = select('#', ...), ...}`.
//! Luau has no such local, so FAF code like `pcall(fn, unpack(arg))` fails.
//!
//! Strategy: find `arg` identifier tokens lexically (skipping strings, comments,
//! and field accesses such as `t.arg`), parse with `full_moon` to get every
//! function body's span, and for each `arg` token mark the innermost enclosing
//! vararg function (unless an inner function declares its own `arg` parameter).
//! Marked functions get `local arg = table.pack(...);` spliced in right after
//! the parameter list — on the same line, so line numbers in errors are kept.
//! Only functions that actually use `arg` pay for the table allocation.

use full_moon::LuaVersion;
use full_moon::ast::{FunctionBody, Parameter};
use full_moon::node::Node;
use full_moon::tokenizer::TokenType;
use full_moon::visitors::Visitor;

use super::lexscan::{long_bracket_level, skip_comment, skip_long_bracket, skip_qstring};

const PRELUDE: &str = " local arg = table.pack(...);";

pub fn rewrite(source: &str) -> String {
    let uses = scan_arg_uses(source);
    if uses.is_empty() || !source.contains("...") {
        return source.to_owned();
    }

    let version = LuaVersion::lua53().with_luau();
    let result = full_moon::parse_fallible(source, version);
    if !result.errors().is_empty() {
        tracing::warn!(
            "lua_translation: full_moon parse failed during vararg rewrite \
             ({} error(s)); leaving source unchanged: {}",
            result.errors().len(),
            result.errors().first().map(|e| e.to_string()).unwrap_or_default(),
        );
        return source.to_owned();
    }

    let mut collector = Collector::default();
    collector.visit_ast(result.ast());
    let funcs = collector.funcs;

    let mut inserts: Vec<usize> = Vec::new();
    for pos in uses {
        // Innermost-first: smallest enclosing span wins.
        let mut enclosing: Vec<&Func> = funcs
            .iter()
            .filter(|f| f.start <= pos && pos < f.end)
            .collect();
        enclosing.sort_unstable_by_key(|f| f.end - f.start);
        for f in enclosing {
            if f.has_arg_param {
                break;
            }
            if f.is_vararg {
                inserts.push(f.insert_at);
                break;
            }
        }
    }
    if inserts.is_empty() {
        return source.to_owned();
    }
    inserts.sort_unstable();
    inserts.dedup();

    let mut out = String::with_capacity(source.len() + inserts.len() * PRELUDE.len());
    let mut last = 0;
    for at in inserts {
        out.push_str(&source[last..at]);
        out.push_str(PRELUDE);
        last = at;
    }
    out.push_str(&source[last..]);
    out
}

struct Func {
    /// Byte span from the opening parameter paren to the end of `end`.
    start: usize,
    end: usize,
    /// Byte just past the closing parameter paren (or return type annotation).
    insert_at: usize,
    is_vararg: bool,
    has_arg_param: bool,
}

#[derive(Default)]
struct Collector {
    funcs: Vec<Func>,
}

impl Visitor for Collector {
    fn visit_function_body(&mut self, body: &FunctionBody) {
        let (open, close) = body.parameters_parentheses().tokens();
        let Some((_, end)) = body.range() else { return };
        let mut insert_at = close.token().end_position().bytes();
        if let Some(ret) = body.return_type()
            && let Some((_, e)) = ret.range() {
            insert_at = e.bytes();
        }
        let mut is_vararg = false;
        let mut has_arg_param = false;
        for p in body.parameters() {
            match p {
                Parameter::Ellipsis(_) => is_vararg = true,
                Parameter::Name(tok) => {
                    if let TokenType::Identifier { identifier } = tok.token_type()
                        && identifier.as_str() == "arg" {
                        has_arg_param = true;
                    }
                }
                _ => {}
            }
        }
        self.funcs.push(Func {
            start: open.token().start_position().bytes(),
            end: end.bytes(),
            insert_at,
            is_vararg,
            has_arg_param,
        });
    }
}

/// Byte offsets of every `arg` identifier outside strings/comments that isn't a
/// field access (`t.arg`, `t:arg`).
fn scan_arg_uses(source: &str) -> Vec<usize> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' | b'\'' => i = skip_qstring(b, i),
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_comment(b, i),
            b'[' if long_bracket_level(b, i) >= 0 => {
                i = skip_long_bracket(b, i, long_bracket_level(b, i) as usize)
            }
            _ if is_ident(c) => {
                let start = i;
                while i < b.len() && is_ident(b[i]) {
                    i += 1;
                }
                if &b[start..i] == b"arg" {
                    let prev = b[..start].iter().rposition(|c| !c.is_ascii_whitespace());
                    let field = match prev {
                        // `..` is concatenation, a single `.` is a field access.
                        Some(p) if b[p] == b'.' => p == 0 || b[p - 1] != b'.',
                        Some(p) => b[p] == b':',
                        None => false,
                    };
                    if !field {
                        out.push(start);
                    }
                }
            }
            _ => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> mlua::Result<String> {
        let lua = mlua::Lua::new();
        lua.load(rewrite(src)).eval()
    }

    #[test]
    fn safecall_style_unpack_arg() {
        let src = "local function f(a, ...) return table.concat(arg, ',') .. ':' .. arg.n end\nreturn f(0, 'x', 'y')";
        assert_eq!(run(src).unwrap(), "x,y:2");
    }

    #[test]
    fn untouched_when_arg_unused() {
        let src = "local function f(...) return select('#', ...) end";
        assert_eq!(rewrite(src), src);
    }

    #[test]
    fn non_vararg_function_untouched() {
        let src = "local function f(x) return arg end";
        assert_eq!(rewrite(src), src);
    }

    #[test]
    fn arg_parameter_is_not_shadowed() {
        let src = "local function f(arg, ...) return arg end";
        assert_eq!(rewrite(src), src);
    }

    #[test]
    fn field_access_and_strings_ignored() {
        let src = "local function f(...) local t = {} return t.arg, 'arg' end -- arg";
        assert_eq!(rewrite(src), src);
    }

    #[test]
    fn inner_closure_uses_outer_arg_as_upvalue() {
        let src = "local function f(...) local g = function() return arg.n end return g() end\nreturn tostring(f(1, 2, 3))";
        assert_eq!(run(src).unwrap(), "3");
    }

    #[test]
    fn concat_before_arg_counts_as_use() {
        let src = "local function f(...) return 'n=' .. arg.n end\nreturn f(1)";
        assert_eq!(run(src).unwrap(), "n=1");
    }

    #[test]
    fn line_numbers_preserved() {
        let src = "function f(...)\n  return arg\nend";
        assert_eq!(rewrite(src).lines().count(), src.lines().count());
    }
}
