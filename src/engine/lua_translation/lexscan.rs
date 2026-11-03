//! Byte-level Lua lexical scanners shared by the translation passes.
//!
//! These let a pass skip over string literals (quoted and long-bracket) and
//! comments so it never rewrites tokens that merely *look* like operators or
//! keywords inside text. Used by `bitwise::scan_for_bitwise` and
//! `continue_mask`; [`tokens`] splits a script into coarse tokens for the
//! for-in pass, which recognises loop headers without a full parse.

/// If `b[i]` opens a long bracket (`[`, `[=[`, `[==[`, …), returns its level
/// (number of `=`); otherwise `-1`.
pub fn long_bracket_level(b: &[u8], i: usize) -> i32 {
    if b.get(i) != Some(&b'[') { return -1; }
    let mut j = i + 1;
    while b.get(j) == Some(&b'=') { j += 1; }
    if b.get(j) == Some(&b'[') { (j - i - 1) as i32 } else { -1 }
}

/// Returns the index just past a quoted string starting at `start`.
pub fn skip_qstring(b: &[u8], start: usize) -> usize {
    let delim = b[start];
    let mut i = start + 1;
    while i < b.len() {
        if b[i] == b'\\' { i += 2; continue; }
        if b[i] == delim { return i + 1; }
        i += 1;
    }
    b.len()
}

/// Returns the index just past a long-bracket string/comment body of `level`
/// opening at `start`.
pub fn skip_long_bracket(b: &[u8], start: usize, level: usize) -> usize {
    let mut i = start + 2 + level;
    'outer: while i < b.len() {
        if b[i] == b']' {
            let mut j = i + 1;
            for _ in 0..level {
                if b.get(j) != Some(&b'=') { i += 1; continue 'outer; }
                j += 1;
            }
            if b.get(j) == Some(&b']') { return j + 1; }
        }
        i += 1;
    }
    b.len()
}

/// Returns the index just past a comment starting at `start` (`--` already
/// confirmed). Handles both line comments and long-bracket block comments.
pub fn skip_comment(b: &[u8], start: usize) -> usize {
    let mut i = start + 2;
    if b.get(i) == Some(&b'[') {
        let lvl = long_bracket_level(b, i);
        if lvl >= 0 { return skip_long_bracket(b, i, lvl as usize); }
    }
    while i < b.len() && b[i] != b'\n' { i += 1; }
    i
}

/// A token's kind, as far as the token-level passes need it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A name or keyword (`text` tells which).
    Name,
    /// A quoted or long-bracket string.
    Str,
    Number,
    /// One of `( ) [ ] { } , . :`.
    Punct(u8),
    /// Any other operator (`..`, `...`, `==`, `+`, `#`, ...).
    Op,
}

/// The tokens of `src` (comments and whitespace skipped), with their byte
/// ranges. Coarse: operators other than the punctuation in [`Tok::Punct`]
/// are lumped together, which is all a pass that recognises expression
/// shapes needs.
pub fn tokens(src: &str) -> Vec<(Tok, usize, usize)> {
    let b = src.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 4);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start = i;
        let kind = match c {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => {
                i += 1;
                continue;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                i = skip_comment(b, i);
                continue;
            }
            b'"' | b'\'' => {
                i = skip_qstring(b, i);
                Tok::Str
            }
            b'[' if long_bracket_level(b, i) >= 0 => {
                i = skip_long_bracket(b, i, long_bracket_level(b, i) as usize);
                Tok::Str
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                Tok::Name
            }
            b'0'..=b'9' => {
                i = skip_number(b, i);
                Tok::Number
            }
            b'.' if b.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                i = skip_number(b, i);
                Tok::Number
            }
            b'.' if b.get(i + 1) == Some(&b'.') => {
                i += if b.get(i + 2) == Some(&b'.') { 3 } else { 2 };
                Tok::Op
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b'.' | b':' => {
                i += 1;
                Tok::Punct(c)
            }
            _ => {
                i += 1;
                Tok::Op
            }
        };
        out.push((kind, start, i));
    }
    out
}

/// Returns the index just past a numeric literal starting at `start`
/// (decimal or hex, with a fraction and exponent).
fn skip_number(b: &[u8], start: usize) -> usize {
    let hex = b[start] == b'0' && matches!(b.get(start + 1), Some(b'x' | b'X'));
    let mut i = if hex { start + 2 } else { start };
    while i < b.len() {
        let c = b[i];
        let exponent = if hex { matches!(c, b'p' | b'P') } else { matches!(c, b'e' | b'E') };
        if exponent && matches!(b.get(i + 1), Some(b'+' | b'-')) {
            i += 2;
        } else if c.is_ascii_alphanumeric() || c == b'.' || c == b'_' {
            i += 1;
        } else {
            break;
        }
    }
    i
}
