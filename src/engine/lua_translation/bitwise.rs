//! Rewrites Lua 5.3 bitwise operators to `bit32.*` calls for Luau compatibility.
//!
//! | Lua 5.3   | Luau equivalent       |
//! |-----------|----------------------|
//! | `a & b`   | `bit32.band(a, b)`   |
//! | `a \| b`  | `bit32.bor(a, b)`    |
//! | `a ~ b`   | `bit32.bxor(a, b)`   |
//! | `~a`      | `bit32.bnot(a)`      |
//! | `a << b`  | `bit32.lshift(a, b)` |
//! | `a >> b`  | `bit32.rshift(a, b)` |
//!
//! Strategy: parse with `full_moon` (Lua 5.3 + Luau enabled together, so the
//! same grammar accepts both bitwise operators *and* Luau's `continue`), find
//! every bitwise expression, and splice `bit32.*` calls over their byte ranges
//! in the original source. Everything outside a bitwise expression — including
//! strings, comments, and formatting — is left byte-for-byte untouched, because
//! `full_moon` correctly classifies those as trivia rather than operators.

use full_moon::LuaVersion;
use full_moon::ast::{BinOp, Expression, UnOp};
use full_moon::node::Node;
use full_moon::visitors::Visitor;

use super::lexscan::{long_bracket_level, skip_comment, skip_long_bracket, skip_qstring};

pub fn rewrite(source: &str) -> String {
    // Cheap pre-scan: skip parsing entirely unless a real bitwise token exists
    // outside of strings/comments (the overwhelmingly common case).
    if !scan_for_bitwise(source) {
        return source.to_owned();
    }

    let version = LuaVersion::lua53().with_luau();
    let result = full_moon::parse_fallible(source, version);
    if !result.errors().is_empty() {
        // Fail-safe: on any parse error, byte positions are unreliable, so leave
        // the source untouched rather than risk corrupting it.
        tracing::warn!(
            "lua_translation: full_moon parse failed during bitwise rewrite \
             ({} error(s)); leaving source unchanged: {}",
            result.errors().len(),
            result.errors().first().map(|e| e.to_string()).unwrap_or_default(),
        );
        return source.to_owned();
    }

    let mut collector = Collector::default();
    collector.visit_ast(result.ast());
    splice(source, collector.maximal())
}

// ─── AST collection ────────────────────────────────────────────────────────────

#[derive(Default)]
struct Collector {
    /// `(start_byte, end_byte, expr)` for every bitwise-rooted expression.
    found: Vec<(usize, usize, Expression)>,
}

impl Visitor for Collector {
    fn visit_expression(&mut self, expr: &Expression) {
        if is_bitwise_root(expr)
            && let Some((start, end)) = expr.range() {
            self.found.push((start.bytes(), end.bytes(), expr.clone()));
        }
    }
}

impl Collector {
    /// Keep only the outermost bitwise expressions — any range strictly
    /// contained within another is rewritten recursively by [`render`], so
    /// splicing it separately would double-rewrite.
    fn maximal(self) -> Vec<(usize, usize, Expression)> {
        let ranges: Vec<(usize, usize)> = self.found.iter().map(|(s, e, _)| (*s, *e)).collect();
        self.found
            .iter()
            .enumerate()
            .filter(|(i, (s, e, _))| {
                !ranges.iter().enumerate().any(|(j, (os, oe))| {
                    j != *i && os <= s && e <= oe && (os, oe) != (s, e)
                })
            })
            .map(|(_, item)| item.clone())
            .collect()
    }
}

/// Whether an expression's root operation is a bitwise one we rewrite. Sees
/// through parentheses so `(a | b)` counts as bitwise-rooted.
fn is_bitwise_root(expr: &Expression) -> bool {
    match expr {
        Expression::BinaryOperator { binop, .. } => bit_fn(binop).is_some(),
        Expression::UnaryOperator { unop: UnOp::Tilde(_), .. } => true,
        Expression::Parentheses { expression, .. } => is_bitwise_root(expression),
        _ => false,
    }
}

/// The `bit32` function name for a bitwise binary operator, or `None`.
fn bit_fn(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Ampersand(_) => Some("band"),
        BinOp::Pipe(_) => Some("bor"),
        BinOp::Tilde(_) => Some("bxor"),
        BinOp::DoubleLessThan(_) => Some("lshift"),
        BinOp::DoubleGreaterThan(_) => Some("rshift"),
        _ => None,
    }
}

// ─── rendering ───────────────────────────────────────────────────────────────

/// Renders an expression to its Luau form. Bitwise operations become `bit32.*`
/// calls; every non-bitwise sub-expression is copied verbatim from the original
/// source so its formatting and comments survive unchanged.
fn render(src: &str, expr: &Expression) -> String {
    match expr {
        Expression::BinaryOperator { lhs, binop, rhs } => match bit_fn(binop) {
            Some(f) => format!("bit32.{}({}, {})", f, render(src, lhs), render(src, rhs)),
            None => verbatim(src, expr),
        },
        Expression::UnaryOperator { unop: UnOp::Tilde(_), expression } => {
            format!("bit32.bnot({})", render(src, expression))
        }
        // Drop redundant parens around a bitwise op — `bit32.*(…)` is already a
        // single atomic term, so the parens are never needed for precedence.
        Expression::Parentheses { expression, .. } if is_bitwise_root(expression) => {
            render(src, expression)
        }
        _ => verbatim(src, expr),
    }
}

/// The exact original source text of an expression (positions exclude trivia).
fn verbatim(src: &str, expr: &Expression) -> String {
    match expr.range() {
        Some((s, e)) => src[s.bytes()..e.bytes()].to_owned(),
        None => expr.to_string(),
    }
}

/// Splices each maximal bitwise expression's `bit32.*` rendering over its byte
/// range. Applied right-to-left so earlier offsets stay valid; maximal ranges
/// are disjoint, so the order is otherwise irrelevant.
fn splice(src: &str, mut items: Vec<(usize, usize, Expression)>) -> String {
    items.sort_by_key(|i| std::cmp::Reverse(i.0));
    let mut out = src.to_owned();
    for (start, end, expr) in items {
        let replacement = render(src, &expr);
        out.replace_range(start..end, &replacement);
    }
    out
}

// ─── pre-scan ────────────────────────────────────────────────────────────────

/// Fast byte scan: is there a bitwise operator outside of strings/comments?
fn scan_for_bitwise(src: &str) -> bool {
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' => i = skip_qstring(b, i),
            b'[' => {
                let lvl = long_bracket_level(b, i);
                if lvl >= 0 { i = skip_long_bracket(b, i, lvl as usize); } else { i += 1; }
            }
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_comment(b, i),
            b'&' | b'|' => return true,
            b'<' if b.get(i + 1) == Some(&b'<') => return true,
            b'>' if b.get(i + 1) == Some(&b'>') => return true,
            b'~' if b.get(i + 1) != Some(&b'=') => return true,
            _ => i += 1,
        }
    }
    false
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_band() {
        assert_eq!(rewrite("local x = a & b"), "local x = bit32.band(a, b)");
    }

    #[test]
    fn simple_bor() {
        assert_eq!(rewrite("local x = a | b"), "local x = bit32.bor(a, b)");
    }

    #[test]
    fn simple_lshift() {
        assert_eq!(rewrite("local x = a << 2"), "local x = bit32.lshift(a, 2)");
    }

    #[test]
    fn simple_rshift() {
        assert_eq!(rewrite("local x = a >> 2"), "local x = bit32.rshift(a, 2)");
    }

    #[test]
    fn simple_bxor() {
        assert_eq!(rewrite("local x = a ~ b"), "local x = bit32.bxor(a, b)");
    }

    #[test]
    fn simple_bnot() {
        assert_eq!(rewrite("local x = ~a"), "local x = bit32.bnot(a)");
    }

    #[test]
    fn init_lua_affinity() {
        let input = "processAffinityMask = systemAffinityMask & (systemAffinityMask << 2)";
        assert_eq!(
            rewrite(input),
            "processAffinityMask = bit32.band(systemAffinityMask, bit32.lshift(systemAffinityMask, 2))"
        );
    }

    #[test]
    fn debug_function_mantissa() {
        let input = "local mantissa = (op & 0x1f) | (b << 5)";
        assert_eq!(
            rewrite(input),
            "local mantissa = bit32.bor(bit32.band(op, 0x1f), bit32.lshift(b, 5))"
        );
    }

    #[test]
    fn debug_function_mantissa2() {
        let input = "mantissa = mantissa | (c << 14)";
        assert_eq!(rewrite(input), "mantissa = bit32.bor(mantissa, bit32.lshift(c, 14))");
    }

    #[test]
    fn debug_function_if() {
        let input = "if op & 0x20 ~= 0 then end";
        assert_eq!(rewrite(input), "if bit32.band(op, 0x20) ~= 0 then end");
    }

    #[test]
    fn no_change_without_bitwise() {
        let input = "local x = a + b * c";
        assert_eq!(rewrite(input), input);
    }

    #[test]
    fn no_change_ne_operator() {
        let input = "if a ~= b then end";
        assert_eq!(rewrite(input), input);
    }

    #[test]
    fn bitwise_in_string_unchanged() {
        let input = r#"local s = "a & b""#;
        assert_eq!(rewrite(input), input);
    }

    #[test]
    fn bitwise_in_comment_unchanged() {
        let input = "-- R(A) := RK(B) & RK(C)\nlocal x = 1";
        assert_eq!(rewrite(input), input);
    }

    #[test]
    fn doc_comment_type_union_unchanged() {
        // LuaLS `|` type unions live in `---` comments and must not be touched,
        // even when the same file has real bitwise code.
        let input = "---@return number | nil\nlocal x = a | b";
        assert_eq!(rewrite(input), "---@return number | nil\nlocal x = bit32.bor(a, b)");
    }

    #[test]
    fn coexists_with_continue() {
        // `continue` (Luau) and bitwise ops (Lua 5.3) in one chunk — the parser
        // accepts both, and `continue` round-trips untouched.
        let input = "for i = 1, 10 do\n    if i & 1 == 0 then continue end\nend";
        assert_eq!(
            rewrite(input),
            "for i = 1, 10 do\n    if bit32.band(i, 1) == 0 then continue end\nend"
        );
    }

    #[test]
    fn nested_in_call_args() {
        assert_eq!(rewrite("foo(a & b, c)"), "foo(bit32.band(a, b), c)");
    }

    #[test]
    fn xor_not_confused_with_ne() {
        assert_eq!(rewrite("local x = a ~ b ~= c"), "local x = bit32.bxor(a, b) ~= c");
    }
}
