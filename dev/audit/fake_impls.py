#!/usr/bin/env python3
"""Fake-implementation audit (docs/guides/audits.md).

The bridge audit (api_audit.py) sees placeholders only when they are the
registered stubs (`api/pending.rs`, `ui/user_api.rs` register_stubs /
register_method_stubs). This finds the other kind: functions that are
registered as if implemented but return a constant or ignore their
arguments, in

  * Lua embedded in the engine: `include_str!` .lua files under src/ and
    `r#"..."#` / `r"..."` chunks in .rs files (tests skipped);
  * Rust registrations: closures passed to `lua.create_function`,
    `moho::add_method(lua, "<class>", "<Name>", ...)`, `add(v, "Name", ...)`
    and friends, including names fed by `for name in [...]` loops.

Each candidate is ranked by its call sites in FAF's Lua (../fa), split by
layer the way api_audit.py does.

    python3 dev/audit/fake_impls.py            # table, by FAF calls
    python3 dev/audit/fake_impls.py --tsv      # tab-separated, for sorting
    python3 dev/audit/fake_impls.py --no-ignores  # drop ignores-args hits
    python3 dev/audit/fake_impls.py --hide-ok     # drop KNOWN_OK names

Kinds:
  empty        body does nothing (Lua `end` / `return`; Rust `Ok(())`)
  const        returns only literals (nil/bool/number/string/{}/Vector(0,0,0))
  arg          returns one of its arguments unchanged
  ignores-args (Rust) every argument is `_`-ignored but the body does more
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import api_audit  # noqa: E402  (FAF source walk and call counter)

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
SRC = os.path.join(ROOT, 'src')

# Names known to be fine although they look fake: genuine constants or
# moho behaviour that really is a fixed value. name -> reason. Listed in the
# output with state 'ok' (use --hide-ok to drop them).
KNOWN_OK = {
    'GetSimTicksPerSecond': 'moho runs the sim at 10 ticks/s',
    'GetAntiAliasingOptions': 'option list, not state',
    'EnumColorNames': 'fixed list of colour names',
    'DebugFacilitiesEnabled': 'release moho returns false',
    'GpgNetActive': 'no FAF client: false is correct offline',
    'GpgNetSend': 'no FAF client to send to',
    # One object of its kind, so the ignored receiver / position is right.
    'Hide': 'cursor: one cursor', 'Show': 'cursor: one cursor', 'ResetToDefault': 'cursor: one cursor',
    'GetRootFrame': 'one root frame (no multihead)', 'GetNumRootFrames': 'one root frame (no multihead)',
    'GetTopmostWorldViewAt': 'one full-window world view',
    'GetRightMouseButtonOrder': 'one world view; renderer computes it',
    'GetMapWaterRatio': 'map-wide value; the brain is irrelevant',
    'GetNoRushTicks': 'scenario option; the brain is irrelevant',
    'close': 'io file handle shim', 'lines': 'io file handle shim',
    'Update': 'CPrefetchSet: prefetching is a loading hint', 'Reset': 'CPrefetchSet: prefetching is a loading hint',
    'BlueprintLoaderUpdateProgress': 'loading-bar tick; no loading bar yet, nothing else reads it',
    'Dump': 'debug print of a control',
}


# --------------------------------------------------------------------------
# Lua tokenizer and function finder

LUA_KEYWORDS = {'and', 'break', 'do', 'else', 'elseif', 'end', 'false', 'for', 'function', 'if', 'in',
                'local', 'nil', 'not', 'or', 'repeat', 'return', 'then', 'true', 'until', 'while', 'continue'}


def lua_tokens(src):
    """[(kind, text, line)] with kind in name/num/str/op; comments dropped."""
    out, i, n, line = [], 0, len(src), 1
    while i < n:
        c = src[i]
        if c == '\n':
            line += 1
            i += 1
        elif c in ' \t\r':
            i += 1
        elif src.startswith('--', i):
            m = re.match(r'--\[(=*)\[', src[i:])
            if m:
                close = ']' + m.group(1) + ']'
                j = src.find(close, i)
                j = n if j < 0 else j + len(close)
            else:
                j = src.find('\n', i)
                j = n if j < 0 else j
            line += src.count('\n', i, j)
            i = j
        elif c == '[' and re.match(r'\[=*\[', src[i:]):
            m = re.match(r'\[(=*)\[', src[i:])
            close = ']' + m.group(1) + ']'
            j = src.find(close, i)
            j = n if j < 0 else j + len(close)
            out.append(('str', src[i:j], line))
            line += src.count('\n', i, j)
            i = j
        elif c in '"\'`':
            j = i + 1
            while j < n and src[j] != c:
                j += 2 if src[j] == '\\' else 1
            j += 1
            out.append(('str', src[i:j], line))
            line += src.count('\n', i, j)
            i = j
        elif c.isdigit() or (c == '.' and i + 1 < n and src[i + 1].isdigit()):
            m = re.match(r'0[xX][0-9a-fA-F]+|\d*\.?\d+(?:[eE][+-]?\d+)?', src[i:])
            out.append(('num', m.group(0), line))
            i += len(m.group(0))
        elif c.isalpha() or c == '_':
            m = re.match(r'[A-Za-z_]\w*', src[i:])
            out.append(('name', m.group(0), line))
            i += len(m.group(0))
        else:
            m = re.match(r'\.\.\.|\.\.|==|~=|<=|>=|::|[-+*/%^#<>=(){}\[\];:,.]', src[i:])
            t = m.group(0) if m else c
            out.append(('op', t, line))
            i += len(t)
    return out


def match_end(toks, i):
    """Index of the `end` closing the block whose body starts at toks[i]."""
    depth = 1
    while i < len(toks):
        k, t, _ = toks[i]
        if k == 'name':
            if t in ('function', 'if', 'do', 'repeat'):
                depth += 1
            elif t in ('end', 'until'):
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return len(toks) - 1


def literal_expr(toks, i, params):
    """End index (exclusive) of a literal expression at i, and whether it was a
    parameter; None if the expression isn't literal."""
    if i >= len(toks):
        return None
    k, t, _ = toks[i]
    if k in ('num', 'str') or (k == 'name' and t in ('nil', 'true', 'false')):
        return i + 1, None
    if k == 'op' and t == '-' and i + 1 < len(toks) and toks[i + 1][0] == 'num':
        return i + 2, None
    if k == 'name' and t in params:
        if i + 1 < len(toks) and toks[i + 1][1] in ('.', ':', '[', '(', 'or', 'and', '..', '+', '-', '*', '/', '=='):
            return None
        return i + 1, t
    if k == 'name' and t in ('Vector', 'Vector2') and i + 1 < len(toks) and toks[i + 1][1] == '(':
        j = i + 2
        while j < len(toks) and toks[j][1] != ')':
            r = literal_expr(toks, j, ())
            if r is None:
                return None
            j = r[0]
            if toks[j][1] == ',':
                j += 1
        return j + 1, None
    if k == 'op' and t == '{':
        j = i + 1
        used_param = False
        while j < len(toks) and toks[j][1] != '}':
            if toks[j][0] == 'name' and j + 1 < len(toks) and toks[j + 1][1] == '=':
                j += 2
            elif toks[j][1] == '[':
                r = literal_expr(toks, j + 1, ())
                if r is None or toks[r[0]][1] != ']' or toks[r[0] + 1][1] != '=':
                    return None
                j = r[0] + 2
            r = literal_expr(toks, j, params)
            if r is None:
                return None
            j = r[0]
            used_param = used_param or r[1] is not None
            if toks[j][1] in (',', ';'):
                j += 1
        return j + 1, ('{}' if used_param else None)
    return None


def classify_lua_body(toks, params):
    """kind or None for the body tokens of a function."""
    if not toks:
        return 'empty'
    if toks[0][1] != 'return':
        return None
    if len(toks) == 1 or (len(toks) == 2 and toks[1][1] == ';'):
        return 'empty'
    i, kinds = 1, []
    while i < len(toks):
        r = literal_expr(toks, i, params)
        if r is None:
            return None
        i, p = r
        kinds.append(p)
        if i < len(toks) and toks[i][1] == ',':
            i += 1
            continue
        if i < len(toks) and toks[i][1] == ';':
            i += 1
        break
    if i != len(toks):
        return None
    if any(k == '{}' for k in kinds):
        return None  # a table built from the arguments: a constructor
    if any(k for k in kinds):
        return 'arg'
    return 'const'


def lua_functions(src):
    """Yield (name, owner, line, params, body_tokens) for named functions."""
    toks = lua_tokens(src)
    # local aliases of moho class tables: local methods = moho.userunit_methods
    aliases = {}
    for i in range(len(toks) - 4):
        if toks[i][1] == 'local' and toks[i + 2][1] == '=' and toks[i + 3][1] == 'moho' and toks[i + 4][1] == '.':
            aliases[toks[i + 1][1]] = toks[i + 5][1]
    i = 0
    while i < len(toks):
        k, t, line = toks[i]
        if not (k == 'name' and t == 'function'):
            i += 1
            continue
        is_local = i > 0 and toks[i - 1][1] == 'local'
        j = i + 1
        chain = []
        while j < len(toks) and toks[j][0] == 'name' and toks[j][1] not in LUA_KEYWORDS:
            chain.append(toks[j][1])
            if toks[j + 1][1] in ('.', ':'):
                j += 2
            else:
                j += 1
                break
        if not chain:  # anonymous: name from `a.b.Name = function` / `Name = function`
            back = i - 1
            if back >= 0 and toks[back][1] == '=':
                b = back - 1
                rev = []
                while b >= 0 and toks[b][0] == 'name':
                    rev.append(toks[b][1])
                    if b - 1 >= 0 and toks[b - 1][1] == '.':
                        b -= 2
                    else:
                        break
                chain = list(reversed(rev))
                is_local = is_local or (b - 1 >= 0 and toks[b - 1][1] == 'local' and len(chain) == 1)
                in_table = b - 1 >= 0 and toks[b - 1][1] in ('{', ',', ';') and len(chain) == 1
                if in_table:
                    chain = ['<table>'] + chain
        if toks[j][1] != '(':
            i += 1
            continue
        close = j
        while toks[close][1] != ')':
            close += 1
        params = [x[1] for x in toks[j + 1:close] if x[0] == 'name'] + ['self']
        end = match_end(toks, close + 1)
        body = toks[close + 1:end]
        if chain and not is_local:
            owner = '.'.join(chain[:-1])
            owner = aliases.get(owner, owner)
            yield chain[-1], owner, line, params, body, src_line_span(toks, i, end)
        i += 1  # nested functions are visited too


def src_line_span(toks, i, end):
    return toks[i][2], toks[end][2] if end < len(toks) else toks[-1][2]


# --------------------------------------------------------------------------
# Sources

def strip_tests(text):
    m = re.search(r'#\[cfg\(test\)\]\s*mod \w+', text)
    return text[:m.start()] if m else text


def rust_files():
    for dp, _, fn in sorted(os.walk(SRC)):
        for f in sorted(fn):
            if f.endswith('.rs') and 'test' not in f:
                yield os.path.join(dp, f)


def shared_modules():
    """engine/api modules the interface VM registers too (ui/mod.rs)."""
    t = open(os.path.join(SRC, 'ui', 'mod.rs')).read()
    return set(re.findall(r'\bapi::(\w+)::', t))


SHARED = None


def layer_of(path):
    global SHARED
    if SHARED is None:
        SHARED = shared_modules()
    rel = os.path.relpath(path, SRC)
    if rel.startswith('ui/'):
        return 'user'
    if rel.startswith('engine/lua/shims/'):
        return 'both'
    m = re.match(r'engine/api/(\w+)', rel)
    if m and m.group(1) in SHARED:
        return 'both'
    if rel.startswith('lua_overrides/'):
        return 'both'
    return 'sim'


def lua_chunks():
    """(path, first_line, text, layer) for every Lua chunk in the engine."""
    for dp, _, fn in sorted(os.walk(SRC)):
        for f in sorted(fn):
            p = os.path.join(dp, f)
            if f.endswith('.lua'):
                yield p, 1, open(p, encoding='utf-8').read(), layer_of(p)
    for p in rust_files():
        text = strip_tests(open(p, encoding='utf-8').read())
        for m in re.finditer(r'r(#*)"(.*?)"\1', text, re.S):
            body = m.group(2)
            if 'function' not in body:
                continue
            yield p, text.count('\n', 0, m.start(2)) + 1, body, layer_of(p)


def comment_above(lines, idx, comment):
    """Comment block attached to the definition at lines[idx] (0-based):
    walks up over sibling one-line definitions to the nearest comment block."""
    j, steps = idx - 1, 0
    indent = len(lines[idx]) - len(lines[idx].lstrip())
    while j >= 0 and steps < 12:
        s = lines[j].strip()
        if s.startswith(comment):
            block = []
            while j >= 0 and lines[j].strip().startswith(comment):
                block.append(lines[j].strip().lstrip(comment[0]).strip())
                j -= 1
            return ' '.join(reversed(block))
        one_liner = (len(lines[j]) - len(lines[j].lstrip()) == indent and
                     (re.match(r'(function |methods\.|\w+\.\w+\s*=|g\.set\(|add\(|crate::|moho::|lua\.globals)', s)) and
                     (s.endswith('end') or s.endswith(';') or s.endswith('end,')))
        if not one_liner:
            return ''
        j -= 1
        steps += 1
    return ''


def scan_lua():
    for path, first, text, layer in lua_chunks():
        lines = text.split('\n')
        for name, owner, line, params, body, (l0, l1) in lua_functions(text):
            if name.startswith('_'):
                continue
            kind = classify_lua_body(body, params)
            if not kind:
                continue
            doc = comment_above(lines, line - 1, '--')
            inner = ' '.join(lines[l0 - 1:l1]).strip()
            if '--' in inner:
                doc = (doc + ' ' + inner.split('--', 1)[1]).strip()
            is_method = bool(owner) and owner not in ('<table>',)
            cls = owner if is_method else '<global>'
            if owner == '<table>':
                cls = '<table field>'
            yield dict(name=name, cls=cls, layer=layer, lang='lua', kind=kind,
                       file=os.path.relpath(path, ROOT), line=first + line - 1,
                       body=re.sub(r'\s+', ' ', ' '.join(lines[l0 - 1:min(l1, l0 + 2)]).strip())[:110],
                       doc=doc)


# --------------------------------------------------------------------------
# Rust closures

RUST_LIT = re.compile(r'''^(?:\(\)|LuaNil|(?:mlua::|Lua)?Value::Nil|LuaValue::Nil|true|false|None|
    -?\d[\d_]*(?:\.\d+)?(?:f32|f64|i32|i64|u32|u64|usize)?|"[^"]*"|String::new\(\)|Vec::(?:<.*>::)?new\(\)|vec!\[\]|
    LuaValue::Boolean\((?:true|false)\)|LuaValue::(?:Integer|Number)\(-?[\d.]+\))$''', re.X)


def skip_rust_string(text, i):
    if text[i] == '"':
        j = i + 1
        while j < len(text) and text[j] != '"':
            j += 2 if text[j] == '\\' else 1
        return j + 1
    m = re.match(r'r(#*)"', text[i:])
    if m:
        close = '"' + m.group(1)
        return text.find(close, i + len(m.group(0))) + len(close)
    return None


def rust_closure_end(text, i, block):
    """End of a closure body at i: the matching `}` of a block body, else
    the `,` / unmatched closer ending an expression body."""
    depth = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == '/' and text.startswith('//', i):
            i = text.find('\n', i)
            continue
        if c == '"' or (c == 'r' and re.match(r'r#*"', text[i:]) and not re.match(r'\w', text[i - 1])):
            i = skip_rust_string(text, i)
            continue
        if c == "'" and re.match(r"'(\\.|[^\\'])'", text[i:]):
            i += len(re.match(r"'(\\.|[^\\'])'", text[i:]).group(0))
            continue
        if c in '([{':
            depth += 1
        elif c in ')]}':
            depth -= 1
            if depth < 0:
                return i
            if depth == 0 and block:
                return i
        elif c == ',' and depth == 0 and not block:
            return i
        i += 1
    return n


def split_args(args):
    """Argument patterns of a closure `|a, (b, c): (T, U)|` -> ['a', 'b', 'c']."""
    pats, depth, cur = [], 0, ''
    for c in args:
        if c in '(<[':
            depth += 1
        elif c in ')>]':
            depth -= 1
        if c == ',' and depth == 0:
            pats.append(cur)
            cur = ''
        else:
            cur += c
    pats.append(cur)
    names = []
    for p in pats:
        pat = p.split(':', 1)[0]
        names += re.findall(r'[A-Za-z_]\w*', pat)
    return names


def resolve_name(window, text_before, lines_before):
    """Registered name (or names) for a closure, from the call around it."""
    m = re.search(r'(?:add_method\(\s*\w+\s*,\s*[^,]+,|\badd\(\s*\w+\s*,|\.(?:set|raw_set)\()\s*("?)(\w+)"?\s*,', window)
    if m and m.group(1):
        return [m.group(2)], None
    if not m:
        lits = re.findall(r'"([A-Za-z_]\w*)"', window)
        return (lits[-1:], None) if lits else ([], None)
    var = m.group(2)
    # for var in [..] / for (var, ..) in [..] / for &var in LIST
    for lm in reversed(list(re.finditer(r'for\s+(?:&|\()?\s*' + re.escape(var) + r'\b[^\n]*?\bin\s+(&?\[[^\]]*\]|[A-Z_]+)', text_before))):
        arr = lm.group(1)
        if arr.lstrip('&').startswith('['):
            if lm.group(0).lstrip().startswith('for (') or re.match(r'for\s+\(', lm.group(0)):
                names = re.findall(r'\(\s*"([A-Za-z_]\w*)"', arr)
            else:
                names = re.findall(r'"([A-Za-z_]\w*)"', arr)
            return names, lm.start()
        if arr in ('USER_GLOBALS', 'CORE_GLOBALS', 'PENDING'):
            return [], None  # the registered stubs: api_audit.py's job
        cm = re.search(r'const\s+' + arr + r'\s*:[^=]*=\s*&?\[(.*?)\];', text_before, re.S)
        return (re.findall(r'"([A-Za-z_]\w*)"', cm.group(1)) if cm else ['<' + arr + '>']), lm.start()
    return [], None


def class_of(window, text_before):
    m = re.search(r'add_method\(\s*\w+\s*,\s*"(\w+)"', window)
    if m:
        return m.group(1)
    m = re.search(r'add_method\(\s*\w+\s*,\s*(\w+)\s*,', window) or re.search(r'\badd\(\s*(\w+)\s*,', window)
    if m:
        var = m.group(1)
        lm = list(re.finditer(r'let\s+' + re.escape(var) + r'\s*(?::[^=]+)?=\s*"(\w+)"', text_before))
        if lm:
            return lm[-1].group(1)
        lm = list(re.finditer(r'for\s+\(?[^)]*\b' + re.escape(var) + r'\b[^\n]*\bin\s+\[([^\]]*)\]', text_before))
        if lm:
            return '|'.join(sorted(set(re.findall(r'"(\w+_methods|\w+)"', lm[-1].group(1)))))[:40] or var
        return var
    m = re.search(r'class_table\(\s*\w+\s*,\s*"(\w+)"\s*\)\?\s*\.(?:raw_)?set', window)
    if m:
        return m.group(1)
    if re.search(r'globals\(\)|\bg\.(?:raw_)?set|\bglobals\.(?:raw_)?set', window):
        return '<global>'
    return '?'


def mask_rust(text):
    """text with comments and string contents blanked (same length, newlines
    kept), for finding statement boundaries."""
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        if text.startswith('//', i):
            j = text.find('\n', i)
            j = n if j < 0 else j
        elif text[i] == '"' or (text[i] == 'r' and re.match(r'r#*"', text[i:]) and not re.match(r'\w', text[i - 1])):
            j = skip_rust_string(text, i)
            i += 1  # keep the opening quote
            j -= 1
        else:
            i += 1
            continue
        for k in range(i, j):
            if out[k] != '\n':
                out[k] = ' '
        i = j + 1
    return ''.join(out)


def strip_rust_comments(s):
    return re.sub(r'//[^\n]*', '', s)


def scan_rust(include_ignores):
    for path in rust_files():
        raw = open(path, encoding='utf-8').read()
        text = strip_tests(raw)
        lines = text.split('\n')
        masked = mask_rust(text)
        for m in re.finditer(r'(?<=[(,])\s*(move\s+)?\|([^|]*)\|', masked):
            start = m.start()
            window_start = max(masked.rfind(';', 0, start), masked.rfind('{\n', 0, start), masked.rfind('}\n', 0, start)) + 1
            window = text[window_start:start]
            if not re.search(r'create_function|add_method|create_async_function', window):
                continue
            names, for_pos = resolve_name(window, text[:window_start], None)
            if not names:
                continue
            args = split_args(m.group(2))
            after = m.end()
            rt = re.match(r'\s*->\s*[^{]+', text[after:])
            if rt:
                after += len(rt.group(0))
            body_start = after + len(text[after:]) - len(text[after:].lstrip())
            block = text[body_start] == '{'
            end = rust_closure_end(text, body_start, block)
            body = text[body_start + 1:end] if block else text[body_start:end]
            core = strip_rust_comments(body).strip().rstrip(';').strip()
            kind = None
            if re.fullmatch(r'(?:Ok\()?\w+\.create_table\(\)\??\)?', core):
                core = 'Ok({})'
            mm = re.fullmatch(r'Ok\((.*)\)', core, re.S)
            if mm:
                val = mm.group(1).strip()
                parts = [p.strip() for p in re.sub(r'^\((.*)\)$', r'\1', val).split(',')] if val.startswith('(') and val != '()' else [val]
                user_args = [a for a in args[1:] if not a.startswith('_')]
                if val == '{}' or all(RUST_LIT.match(p) for p in parts if p):
                    kind = 'empty' if val in ('()', 'LuaNil', 'LuaValue::Nil') else 'const'
                elif len(parts) == 1 and parts[0] in user_args:
                    kind = 'self' if parts[0] == user_args[0] and parts[0] in ('this', 'self') else 'arg'
            if not kind and include_ignores:
                lua_arg = args[0] if args else ''
                rest = args[1:]
                if rest and all(a.startswith('_') for a in rest) and '()' not in m.group(2).split(',', 1)[-1]:
                    kind = 'ignores-args'
            if not kind:
                continue
            line_no = text.count('\n', 0, start) + 1
            lead = re.match(r'(?:\s*//[^\n]*\n)*\s*', window).end()  # skip blank and comment lines
            stmt_line = text.count('\n', 0, window_start + lead) + 1
            if for_pos is not None:
                stmt_line = text.count('\n', 0, for_pos) + 1
            doc = comment_above(lines, stmt_line - 1, '//')
            inner = re.findall(r'//\s*([^\n]*)', body)
            if inner:
                doc = (doc + ' ' + ' '.join(inner)).strip()
            cls = class_of(window, text[:window_start])
            snippet = re.sub(r'\s+', ' ', text[m.start():min(end + 1, m.start() + 110)]).strip()
            for name in names:
                yield dict(name=name, cls=cls, layer=layer_of(path), lang='rust', kind=kind,
                           file=os.path.relpath(path, ROOT), line=line_no, body=snippet[:110], doc=doc)


# --------------------------------------------------------------------------

def stub_names():
    t = open(os.path.join(SRC, 'ui', 'user_api.rs')).read()
    p = open(os.path.join(SRC, 'engine', 'api', 'pending.rs')).read()
    names = {}
    for const, src in (('USER_GLOBALS', t), ('CORE_GLOBALS', t), ('PENDING', p)):
        m = re.search(const + r': &\[&str\] = &\[(.*?)\];', src, re.S)
        for n in re.findall(r'"(\w+)"', m.group(1) if m else ''):
            names.setdefault(n, const)
    return names


def main():
    args = set(sys.argv[1:])
    rows = list(scan_lua()) + list(scan_rust('--no-ignores' not in args))
    stubs = stub_names()
    texts = api_audit.faf_sources()
    cache = {}
    # Homonyms: above 1, the call count is an upper bound. For a method, how
    # many FAF classes declare that name; for a global, 1 + FAF's own
    # functions of that name.
    declared = {}
    for (_, cls), names in api_audit.faf_api().items():
        if cls != '<global>':
            for n in names:
                declared[n] = declared.get(n, 0) + 1
    # ... and FAF's own Lua classes defining a method of that name
    # (maui's Text:SetColor, Lua-side Play/Stop helpers).
    for _, t in texts:
        for n in re.findall(r'function\s+\w+[:.](\w+)\s*\(|\b(\w+)\s*=\s*function\s*\(\s*self\b', t):
            n = n[0] or n[1]
            declared[n] = declared.get(n, 0) + 1
    # Globals: FAF files defining a function of the same name (lobby.lua's
    # own IsObserver(id), ModsManager's SavePreferences): calls may be to it.
    faf_defs = {}
    for _, t in texts:
        for n in set(re.findall(r'(?:^|[^\w.:])function\s+(\w+)\s*\(|^\s*(?:local\s+)?(\w+)\s*=\s*function\b', t, re.M)):
            n = n[0] or n[1]
            faf_defs[n] = faf_defs.get(n, 0) + 1

    def count(vm, name, method):
        key = (vm, name, method)
        if key not in cache:
            cache[key] = api_audit.calls(texts, vm, name, method)
        return cache[key]

    seen = set()
    out = []
    for r in rows:
        key = (r['name'], r['cls'], r['file'], r['line'])
        if key in seen or r['name'].startswith('<'):
            continue
        seen.add(key)
        method = r['cls'] not in ('<global>',)
        sim = count('sim', r['name'], method) if r['layer'] in ('sim', 'both') else (0, [])
        ui = count('ui', r['name'], method) if r['layer'] in ('user', 'both') else (0, [])
        r['sim'], r['ui'] = sim[0], ui[0]
        r['faf'] = ', '.join((sim[1] + ui[1])[:3])
        r['classes'] = declared.get(r['name'], 0) if method else 1 + faf_defs.get(r['name'], 0)
        r['stub'] = stubs.get(r['name'], '')
        r['ok'] = KNOWN_OK.get(r['name'], '')
        out.append(r)
    if '--hide-ok' in args:
        out = [r for r in out if not r['ok']]
    out.sort(key=lambda r: (-(r['sim'] + r['ui']), r['layer'], r['cls'], r['name']))
    if '--tsv' in args:
        cols = ['name', 'cls', 'layer', 'lang', 'kind', 'sim', 'ui', 'classes', 'file', 'line', 'stub', 'ok', 'doc', 'body', 'faf']
        print('\t'.join(cols))
        for r in out:
            print('\t'.join(str(r[c]).replace('\t', ' ') for c in cols))
        return
    for r in out:
        tag = 'ok' if r['ok'] else ('doc' if r['doc'] else '-')
        shared = f"x{r['classes']}" if r['classes'] > 1 else ''
        print(f"{r['sim'] + r['ui']:4}{shared:4} sim{r['sim']:4} ui{r['ui']:4}  {r['layer']:4} {r['kind']:5} {tag:3} "
              f"{r['cls'][:26]:26} {r['name']:32} {r['file']}:{r['line']}")
    print(f'# {len(out)} candidates', file=sys.stderr)


if __name__ == '__main__':
    main()
