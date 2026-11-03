#!/usr/bin/env python3
"""Blueprint field audit: which blueprint fields the real engine reads that we
ignore.

Reads the BPFIELD lines dev/audit/bp_dump.lua logs (every field path of the
loaded blueprints, with how many blueprints set it), and sorts each field by
whether its name is a string in the game binaries (MohoEngine.dll and the
exe: moho reflects blueprint fields by name, so the fields the engine reads
are there; fields only FAF's Lua reads usually aren't) and whether it appears
as a string literal in our source. An engine field we never mention is a
candidate gap; FAF's Lua reading it too (`.Name`) is noted, since then it may
be script-side only. FAF's blueprint annotations (`engine/Core/Blueprints`)
are a third source: their description is shown beside moho's, and fields FAF
documents that moho doesn't reflect but whose names are in the binary are
listed apart (engine fields the reflection tables may not cover). Names are
matched, not uses: a common word ("Mesh") can be in the binary for another
reason, so check a gap in the disassembly before implementing it
(docs/research/verifying-against-the-binary.md).

    cargo run -- --headless --ticks 1 --no-ai --script dev/audit/bp_dump.lua
    python3 dev/audit/bp_audit.py game.log > docs/api/blueprint-fields.md

Options: --game DIR (default ../Supreme Commander Forged Alliance),
--fa DIR (default ../fa), --all (also list the fields we read).
"""
import argparse
import glob
import os
import re
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, '..', '..'))
# Tables keyed by names rather than fields (category names are strings in the
# binary for their own reasons).
FREE_FORM = {'CategoriesHash'}
# Under these, field names are FAF's own (effect tables, buffs, enhancements):
# a binary string match there is a coincidence.
FREE_FORM_PARENT = re.compile(r'(^|\.)(Enhancements|EnhancementPresets|Buffs|Effects\[\]|Footfall|Treads|TeleportEffects)(\.|\[|$)')
KIND_ORDER = ['unit', 'projectile', 'prop', 'mesh', 'emitter', 'beam', 'trail', 'other']


def binary_strings(paths, min_len=3):
    """Every printable ASCII run (identifier-like) in the binaries."""
    out = set()
    pat = re.compile(rb'[A-Za-z_][A-Za-z0-9_]{%d,}' % (min_len - 1))
    for p in paths:
        with open(p, 'rb') as f:
            out.update(m.decode() for m in pat.findall(f.read()))
    return out


def reflected_fields(paths):
    """{name: (struct, description)} from moho's reflection tables: a field
    name followed by its description, after the blueprint struct's name
    (`RUnitBlueprintAir`, `RProjectileBlueprint`, ...)."""
    out = {}
    for p in paths:
        with open(p, 'rb') as f:
            runs = [r.decode() for r in re.findall(rb'[\x20-\x7e]{3,}', f.read())]
        owner = None
        for i, r in enumerate(runs[:-1]):
            if re.fullmatch(r'R\w*Blueprint\w*', r):
                owner = r
            elif owner and re.fullmatch(r'[A-Z]\w+', r) and ' ' in runs[i + 1] and r not in out:
                out[r] = (owner, runs[i + 1])
    return out


def our_literals():
    """String literals in our Rust and Lua sources."""
    out = set()
    files = glob.glob(os.path.join(REPO, 'src', '**', '*.rs'), recursive=True) + glob.glob(os.path.join(REPO, 'src', '**', '*.lua'), recursive=True)
    for f in files:
        text = open(f, encoding='utf-8', errors='replace').read()
        out.update(re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', text))
        if f.endswith('.lua'):
            out.update(re.findall(r"'([A-Za-z_][A-Za-z0-9_]*)'", text))
            out.update(re.findall(r'\.([A-Z][A-Za-z0-9_]*)\b', text))
    return out


def derived_names():
    """Lua names of `#[derive(LuaTable)]` struct fields (named by
    `rename_all`, so no literal is written; `rename = ".."` ones are)."""
    out = set()
    conv = {
        'PascalCase': lambda f: ''.join(w[:1].upper() + w[1:] for w in f.split('_')),
        'camelCase': lambda f: f.split('_')[0] + ''.join(w[:1].upper() + w[1:] for w in f.split('_')[1:]),
        'UPPERCASE': lambda f: f.upper(),
    }
    attr = re.compile(r'#\[lua\(([^\]]*)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?struct\s+\w+(?:<[^>]*>)?\s*\{', re.S)
    for f in glob.glob(os.path.join(REPO, 'src', '**', '*.rs'), recursive=True):
        text = open(f, encoding='utf-8', errors='replace').read()
        if 'LuaTable' not in text:
            continue
        for m in attr.finditer(text):
            rule = re.search(r'rename_all\s*=\s*"(\w+)"', m.group(1))
            to_lua = conv.get(rule.group(1)) if rule else (lambda f: f)
            if to_lua is None:
                continue
            depth, i = 1, m.end()
            while depth and i < len(text):
                depth += {'{': 1, '}': -1}.get(text[i], 0)
                i += 1
            for field in re.findall(r'^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:', text[m.end():i], re.M):
                out.add(to_lua(field.rstrip('_')))
    return out


def faf_annotations(fa):
    """{name: [(class, description)]} from FAF's blueprint annotations
    (`engine/Core/Blueprints/*.lua`: `--- description` lines, then
    `---@field Name? type`)."""
    out = {}
    for f in sorted(glob.glob(os.path.join(fa, 'engine', 'Core', 'Blueprints', '*.lua'))):
        owner, doc = None, []
        for line in open(f, encoding='utf-8', errors='replace'):
            line = line.rstrip()
            m = re.match(r'---@class\s+(\w+)', line)
            if m:
                owner, doc = m.group(1), []
                continue
            m = re.match(r'---@field\s+([A-Za-z_]\w*)\??\s', line)
            if m and owner:
                out.setdefault(m.group(1), []).append((owner, ' '.join(d for d in doc if d)))
                doc = []
                continue
            if line.startswith('---') and not line.startswith('---@'):
                doc.append(line[3:].strip())
            elif not line.startswith('---'):
                doc = []
    return out


def faf_lua_reads(fa):
    """Field names FAF's Lua reads (`.Name`), outside the engine annotations."""
    out = set()
    for f in glob.glob(os.path.join(fa, 'lua', '**', '*.lua'), recursive=True):
        out.update(re.findall(r'\.([A-Z][A-Za-z0-9_]*)\b', open(f, encoding='utf-8', errors='replace').read()))
    return out


def read_fields(log):
    """{(kind, path): (count, sample)} and {kind: total} from BPFIELD / BPTOTAL lines."""
    fields, totals = {}, {}
    for line in open(log, encoding='utf-8', errors='replace'):
        m = re.search(r'BPFIELD (\S+) (\S+) (\d+) (.*)$', line)
        if m:
            fields[(m.group(1), m.group(2))] = (int(m.group(3)), m.group(4).strip())
            continue
        m = re.search(r'BPTOTAL (\S+) (\d+)', line)
        if m:
            totals[m.group(1)] = int(m.group(2))
    return fields, totals


def annotation_for(path, annotated):
    """FAF's (class, description) for the field at `path`: of the classes
    declaring that name, the one whose name holds the nearest parent segment
    (`Display.Tarmacs[].FadeOut` -> `UnitBlueprintTarmac`), else the first."""
    cands = annotated.get(leaf(path))
    if not cands:
        return None
    parents = [p.replace('[]', '') for p in path.split('.')[:-1]]
    for seg in reversed(parents):
        stem = seg[:-1] if seg.endswith('s') else seg
        for c in cands:
            if stem and stem.lower() in c[0].lower():
                return c
    return cands[0] if len(cands) == 1 else (cands[0][0] + ' (one of ' + str(len(cands)) + ')', cands[0][1])


def short(text, n=110):
    return text if len(text) <= n else text[:n - 1].rstrip() + '…'


def leaf(path):
    name = path.rsplit('.', 1)[-1]
    return name.replace('[]', '')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('log')
    ap.add_argument('--game', default=os.path.join(REPO, '..', 'Supreme Commander Forged Alliance'))
    ap.add_argument('--fa', default=os.path.join(REPO, '..', 'fa'))
    ap.add_argument('--all', action='store_true')
    args = ap.parse_args()

    bins = [p for p in (os.path.join(args.game, 'bin', n) for n in ('MohoEngine.dll', 'SupremeCommander.exe', 'ForgedAlliance.exe')) if os.path.exists(p)]
    if not bins:
        sys.exit(f'no game binaries under {args.game}/bin')
    engine = binary_strings(bins)
    # Both binaries: MohoEngine.dll is the original game's engine, and FA's
    # exe has its own tables with the fields FA added (`HideLifebars`,
    # `IgnoreIfDisabled`); FA's description wins where both have one.
    reflected = reflected_fields([b for b in bins if b.endswith('.dll')])
    reflected.update(reflected_fields([b for b in bins if b.endswith('.exe')]))
    ours = our_literals() | derived_names()
    faf = faf_lua_reads(args.fa)
    annotated = faf_annotations(args.fa)
    fields, totals = read_fields(args.log)
    if not fields:
        sys.exit('no BPFIELD lines (run dev/audit/bp_dump.lua first)')

    # A table counts as read when anything under it is (we read
    # `Display.Mesh.LODs`, not `Mesh` by name).
    read_paths = {(k, p) for (k, p) in fields if leaf(p) in engine and leaf(p) in ours}
    read_prefixes = set()
    for k, p in read_paths:
        parts = p.split('.')
        for i in range(1, len(parts)):
            read_prefixes.add((k, '.'.join(parts[:i])))

    rows = defaultdict(list)
    unreflected = defaultdict(list)
    counts = defaultdict(lambda: defaultdict(int))
    for (kind, path), (n, sample) in fields.items():
        name = leaf(path)
        if not name or path.split('.')[0] in FREE_FORM:
            continue
        if name not in engine:
            counts[kind]['script'] += 1
            continue
        # FAF documents it, moho doesn't reflect it, yet the name is in the
        # binary and we never name it: an engine field the reflection
        # tables may not cover.
        if name in annotated and name not in reflected and name not in ours and (kind, path) not in read_prefixes and not FREE_FORM_PARENT.search(path):
            unreflected[kind].append((n, path, sample, name in faf, annotation_for(path, annotated)))
        state = 'read' if name in ours or (kind, path) in read_prefixes else 'gap' if name in reflected else 'unsure'
        counts[kind][state] += 1
        if state in ('gap', 'unsure') and FREE_FORM_PARENT.search(path):
            continue
        if state == 'gap' or args.all:
            rows[kind].append((n, path, sample, name in faf, state, reflected.get(name), annotation_for(path, annotated)))

    print('# Blueprint fields: engine reads vs ours\n')
    print('Generated by `python3 dev/audit/bp_audit.py game.log` from `dev/audit/bp_dump.lua`. A **gap** is a field the')
    print('loaded blueprints set that moho reflects (its name and description are in the blueprint type tables of')
    print('MohoEngine.dll or FA\'s exe, so the engine reads it) and that our source never names (a literal, or a `LuaTable` derive')
    print('field). Names, not uses: check how the engine uses one in the binary before implementing it.')
    print('"FAF Lua" = FAF\'s scripts also read `.Name`, so it may be handled script-side.\n')
    print(f'Binaries: {", ".join(os.path.basename(b) for b in bins)}.\n')
    print('| kind | blueprints | field paths | engine fields we read | engine fields we ignore | name in binary, not a reflected field | script-only |')
    print('|---|---|---|---|---|---|---|')
    kinds = sorted(set(k for k, _ in fields), key=lambda k: KIND_ORDER.index(k) if k in KIND_ORDER else 99)
    for k in kinds:
        c = counts[k]
        print(f'| {k} | {totals.get(k, "?")} | {sum(c.values())} | {c["read"]} | {c["gap"]} | {c["unsure"]} | {c["script"]} |')
    for k in kinds:
        if not rows[k]:
            continue
        print(f'\n## {k} ({totals.get(k, "?")} blueprints)\n')
        print('| set by | field | sample | FAF Lua | engine struct: description | FAF\'s annotation |' + (' state |' if args.all else ''))
        print('|---|---|---|---|---|---|' + ('---|' if args.all else ''))
        for n, path, sample, in_faf, state, refl, ann in sorted(rows[k], key=lambda r: (-r[0], r[1])):
            desc = f'`{refl[0]}`: {refl[1]}' if refl else ''
            note = (f'`{ann[0]}`: {short(ann[1])}' if ann[1] else f'`{ann[0]}`') if ann else 'not documented'
            cells = [str(n), f'`{path}`', sample, 'yes' if in_faf else '', desc, note] + ([state] if args.all else [])
            print('| ' + ' | '.join(c.replace('|', '\\|') for c in cells) + ' |')
    if any(unreflected.values()):
        print('\n## FAF documents, moho doesn\'t reflect, name in the binary\n')
        print('Fields FAF\'s annotations describe that aren\'t in moho\'s reflection tables, though the name is a string in')
        print('the binary, and that our source never names. The engine may read them another way (or the string is a')
        print('coincidence): check each in the disassembly.\n')
        print('| kind | set by | field | sample | FAF Lua | FAF\'s annotation |')
        print('|---|---|---|---|---|---|')
        for k in kinds:
            for n, path, sample, in_faf, ann in sorted(unreflected[k], key=lambda r: (-r[0], r[1])):
                cells = [k, str(n), f'`{path}`', sample, 'yes' if in_faf else '', f'`{ann[0]}`: {short(ann[1])}' if ann[1] else f'`{ann[0]}`']
                print('| ' + ' | '.join(c.replace('|', '\\|') for c in cells) + ' |')
    set_names = {leaf(p) for (_, p) in fields}
    undocumented = sorted(n for n in reflected if n not in annotated and n in set_names)
    print(f'\n## Moho reflects, FAF doesn\'t document ({len(undocumented)})\n')
    print('Fields the loaded blueprints set that moho\'s reflection tables describe, with no `---@field` in FAF\'s')
    print('blueprint annotations.\n')
    print(', '.join(f'`{n}`' for n in undocumented))


if __name__ == '__main__':
    main()
