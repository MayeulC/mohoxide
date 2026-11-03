#!/usr/bin/env python3
"""Engine API audit (docs/guides/audits.md).

Compares what the sim and interface VMs register (APIDUMP lines from
dev/audit/api_dump.lua) with FAF's engine declarations (../fa/engine), and
ranks each gap by how often FAF's Lua calls it.

    python3 dev/audit/api_audit.py sim.log ui.log

Stubs (pending globals that raise, interface stubs that return nil) count as
present here; the `<stubs>` line only appears with the instrumentation
described in docs/guides/audits.md.
"""
import glob
import os
import re
import sys

FA = os.path.join(os.path.dirname(__file__), '..', '..', '..', 'fa')
UI_DIRS = ('lua/ui/', 'lua/maui/', 'lua/user/', 'lua/options/', 'lua/keymap/', 'lua/modules/',
           'lua/multiplayer/', 'lua/UserSync', 'lua/userInit', 'lua/debug/')
SIM_DIRS = ('lua/sim/', 'lua/AI/', 'lua/aibrains/', 'units/', 'projectiles/', 'props/', 'effects/',
            'lua/SimUtils', 'lua/ScenarioFramework', 'lua/simInit')
# Annotation classes whose runtime table has another name.
RENAMED = {'Camera': 'moho.CameraImpl_methods', 'UserUnit': 'moho.userunit_methods'}


def faf_api():
    """{(layer, class): set(names)} from FAF's /engine annotations."""
    root = os.path.join(FA, 'engine')
    out = {}
    for f in sorted(glob.glob(root + '/**/*.lua', recursive=True)):
        rel = os.path.relpath(f, root)
        if 'Blueprints' in rel:
            continue
        src = open(f, encoding='utf-8', errors='replace').read()
        layer = 'sim' if rel.startswith('Sim') else 'ui' if rel.startswith('User') else 'core'
        if '/' not in rel:
            if rel in ('Sim.lua', 'User.lua', 'Core.lua'):
                for m in re.finditer(r'^function ([A-Za-z_]\w*)\s*\(', src, re.M):
                    out.setdefault((layer, '<global>'), set()).add(m.group(1))
            continue
        locs = {m.group(2): m.group(1) for m in re.finditer(
            r'---@class ([\w.]+)[^\n]*\n(?:---[^\n]*\n)*local (\w+)\s*=\s*\{\}', src)}
        for m in re.finditer(r'^function (\w+)[:.](\w+)\s*\(', src, re.M):
            cls = locs.get(m.group(1), m.group(1))
            out.setdefault((layer, RENAMED.get(cls, cls)), set()).add(m.group(2))
    return out


def load_dump(path):
    d = {}
    for line in open(path, errors='replace'):
        m = re.search(r'APIDUMP (\w+) (\S+) (.*)$', line)
        if m:
            d[m.group(2)] = dict(p.rsplit(':', 1) for p in m.group(3).split(',') if ':' in p)
    return d


def faf_sources():
    texts = []
    for sub in ('lua', 'units', 'projectiles', 'props', 'effects'):
        for dp, _, fn in os.walk(os.path.join(FA, sub)):
            for f in fn:
                if f.endswith('.lua'):
                    p = os.path.join(dp, f)
                    rel = os.path.relpath(p, FA)
                    if not rel.startswith(('lua/tests/', 'lua/web/')):
                        texts.append((rel, open(p, encoding='utf-8', errors='replace').read()))
    return texts


def calls(texts, vm, name, method):
    rx = re.compile((r'[:.]' if method else r'(?<![\w.:])') + re.escape(name) + r'\s*[\(\{"\']')
    n, files = 0, []
    for rel, t in texts:
        if (vm == 'sim' and rel.startswith(UI_DIRS)) or (vm == 'ui' and rel.startswith(SIM_DIRS)):
            continue
        c = len(rx.findall(t))
        if c:
            n += c
            files.append(rel)
    return n, files


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    vms = {'sim': load_dump(sys.argv[1]), 'ui': load_dump(sys.argv[2])}
    texts = faf_sources()
    rows = []
    for (layer, cls), names in sorted(faf_api().items()):
        for vm in (['sim', 'ui'] if layer == 'core' else [layer]):
            have = vms[vm].get(cls, {})
            for name in sorted(names - set(have)):
                n, files = calls(texts, vm, name, cls != '<global>')
                rows.append((vm, cls, name, 'missing', n, files))
    for vm, dump in vms.items():
        for key, state in sorted(dump.get('<stubs>', {}).items()):
            if state != 'STUB':
                continue
            cls, _, name = key.rpartition('.') if key.startswith('moho.') else ('<global>', '', key)
            n, files = calls(texts, vm, name, cls != '<global>')
            rows.append((vm, cls, name, 'stub', n, files))
    for vm, cls, name, kind, n, files in sorted(rows, key=lambda r: (r[0], -r[4], r[1], r[2])):
        print(f'{vm:3} {cls:30} {name:36} {kind:7} {n:4}  {", ".join(files[:2])}')


if __name__ == '__main__':
    main()
