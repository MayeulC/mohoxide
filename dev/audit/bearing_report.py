#!/usr/bin/env python3
"""Tabulates the weapon audit's bearing cases from its log (the AUDIT lines'
`aim` token, dev/audit/weapon_audit.lua's aim_of).

    WEAPON_AUDIT_BEARINGS=all cargo run --release -- --audit-table > table.txt 2> audit.log
    python3 dev/audit/bearing_report.py audit.log            # totals + a table per weapon that misses at some bearing
    python3 dev/audit/bearing_report.py audit.log --all      # a table for every weapon that takes bearings
    python3 dev/audit/bearing_report.py audit.log --screen   # + the systematic-offset and turret-slew screens

An AUDIT line: `AUDIT <id>[@<bearing>] <bp>[<weapon>] vs <target>: <outcome> (fired n, dmg n <stage>, aim b=.. turn=..
arc=.. shot=.. slew=.. n=.. lat=.. long=.. max=..)`. turn: the body's turn from the order to the verdict; arc: the
target inside the weapon's HeadingArc then; shot: ticks from the order to the first shot; slew (structures): the
fewest ticks the turret needs to swing to the bearing at TurretYawSpeed; n / lat / long / max: the case's own
projectile impacts up to the verdict, mean miss across the attacker -> target line (+ towards positive bearings)
and along it (+ beyond), the largest miss.
"""

import collections
import re
import sys

LINE = re.compile(r'AUDIT (\d+)(?:@(-?\d+))? (\S+)\[(\d+)\] vs (\S+): (\S+) \(fired (\d+), dmg (\S+)(.*)\)\s*$')
AIM = re.compile(r'aim ((?:\w+=\S+ ?)+)')


def rows_of(path):
    out = {}
    for line in open(path, encoding='utf-8', errors='replace'):
        i = line.find('AUDIT ')
        if i < 0:
            continue
        m = LINE.search(line[i:].rstrip())
        if not m:
            continue
        rid, b, unit, wi, target, outcome, fired, dmg, stage = m.groups()
        r = dict(id=int(rid), bearing=int(b or 0), unit=unit, weapon=int(wi), target=target, outcome=outcome,
                 fired=int(fired), dmg=dmg)
        a = AIM.search(stage)
        if a:
            for kv in a.group(1).split():
                k, _, v = kv.partition('=')
                r[k] = v
        out[(r['id'], r['bearing'])] = r
    return [out[k] for k in sorted(out, key=lambda k: (k[0], k[1] % 360))]


def main():
    path = sys.argv[1]
    rows = rows_of(path)
    weapons = collections.OrderedDict()
    for r in rows:
        weapons.setdefault(r['id'], []).append(r)
    # only weapons that take bearings (a row off 0)
    weapons = collections.OrderedDict((k, rs) for k, rs in weapons.items() if any(r['bearing'] for r in rs))
    flat = [r for rs in weapons.values() for r in rs]
    totals = collections.Counter(r['outcome'] for r in flat)
    print(f'{len(flat)} cases of {len(weapons)} weapons that take bearings: '
          + ', '.join(f'{k} {v}' for k, v in sorted(totals.items())))
    per_bearing = collections.defaultdict(collections.Counter)
    for r in flat:
        per_bearing[r['bearing'] % 360][r['outcome']] += 1
    for b in sorted(per_bearing):
        c = per_bearing[b]
        print(f'  bearing {b if b <= 180 else b - 360}: pass {c["pass"]}/{sum(c.values())}'
              + ''.join(f', {k} {v}' for k, v in sorted(c.items()) if k != 'pass'))
    print()
    for wid, rs in weapons.items():
        if '--all' not in sys.argv and all(r['outcome'] == 'pass' for r in rs):
            continue
        r0 = rs[0]
        print(f"#### {wid} {r0['unit']} w{r0['weapon']} (target {r0['target']})")
        print()
        print('| bearing | outcome | fired | dmg | body turn | arc | first shot | slew | impacts | mean lateral | mean longitudinal | max miss |')
        print('|---:|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|')
        for r in rs:
            g = r.get
            print(f"| {r['bearing']} | {r['outcome']} | {r['fired']} | {r['dmg']} | {g('turn', '-')} | {g('arc', '-')} | "
                  f"{g('shot', '-')} | {g('slew', '-')} | {g('n', '-')} | {g('lat', '-')} | {g('long', '-')} | {g('max', '-')} |")
        print()
    if '--screen' in sys.argv:
        screen(weapons)
        slew_screen(weapons)


def screen(weapons):
    """Systematic offsets: per weapon with impacts at 4+ bearings, the mean of
    the per-bearing lateral / longitudinal means and the spread of the lateral
    means across bearings (a bearing-dependent aim error shows as a large
    spread; a fixed muzzle / aim-point error as a large mean)."""
    print('Systematic-offset screen (|mean lateral| >= 0.5, lateral spread >= 2 or |mean longitudinal| >= 3):')
    print()
    print('| weapon | bearings | mean lateral | lateral spread (max-min) | mean longitudinal | pass bearings |')
    print('|---|---:|---:|---:|---:|---:|')
    for wid, rs in weapons.items():
        tracked = [r for r in rs if 'lat' in r]
        if len(tracked) < 4:
            continue
        lats = [float(r['lat']) for r in tracked]
        longs = [float(r['long']) for r in tracked]
        mean_lat, mean_long, spread = sum(lats) / len(lats), sum(longs) / len(longs), max(lats) - min(lats)
        if abs(mean_lat) < 0.5 and spread < 2.0 and abs(mean_long) < 3.0:
            continue
        passes = sum(1 for r in rs if r['outcome'] == 'pass')
        print(f"| {wid} {rs[0]['unit']} w{rs[0]['weapon']} | {len(tracked)} | {mean_lat:.2f} | {spread:.2f} | "
              f"{mean_long:.2f} | {passes}/{len(rs)} |")
    print()


def slew_screen(weapons):
    """Turret slew: a structure can't turn its body, so before shooting at a
    target off its nose its turret has to swing there at TurretYawSpeed
    (`slew`, ticks from straight ahead). Cases whose first shot came sooner
    (`shot` < `slew`) fired without turning round."""
    print('Turret-slew screen (structures whose first shot came before the turret could swing to the bearing):')
    print()
    print('| weapon | bearing: first shot / slew ticks |')
    print('|---|---|')
    for wid, rs in weapons.items():
        early = [r for r in rs if 'slew' in r and 'shot' in r and int(r['shot']) < int(r['slew'])]
        if not early:
            continue
        cells = ', '.join(f"{r['bearing']}: {r['shot']} / {r['slew']}" for r in early)
        print(f"| {wid} {rs[0]['unit']} w{rs[0]['weapon']} | {cells} |")
    print()


if __name__ == '__main__':
    main()
