-- dev/audit/targets.lua: the weapon audit's target catalog and per-case
-- policy (dev/audit/weapon_audit.lua reads WEAPON_AUDIT_CATALOG from here;
-- dev/audit/scenario.lua picks the kind per weapon).
-- Canonical route: cat-join this file, dev/audit/scenario.lua and
-- dev/audit/weapon_audit.lua and run the three as one --script;
-- dev/standalone runs rawset these globals as a prelude instead.

-- Surface ships: T1 frigates (the Aeon one carries an anti-torpedo, so Aeon
-- attackers get the UEF frigate).
local ship = { ue = 'ues0103', ua = 'ues0103', ur = 'urs0103', xs = 'xss0103' }

rawset(_G, 'WEAPON_AUDIT_CATALOG', {
    -- Default engagement targets per attacking faction (id prefix).
    fighter = { ue = 'uea0101', ua = 'uaa0101', ur = 'ura0101', xs = 'xsa0101' },
    -- Cheap unarmed (make-sure: no Weapon list) faction structures.
    building = { ue = 'ueb1101', ua = 'uab1101', ur = 'urb1101', xs = 'xsb1101' },
    -- Mobile land units: T2 tanks (Cybran: the T1 Mantis).
    land = { ue = 'uel0201', ua = 'ual0201', ur = 'url0107', xs = 'xsl0201' },
    ship = ship,
    -- the old kind name: an alias of ship
    naval = ship,
    -- FA's T1 submersibles, spawned surfaced.
    sub = { ue = 'ues0203', ua = 'uas0203', ur = 'urs0203', xs = 'xss0203' },
    -- Projectile sources for anti-projectile weapons (not target kinds):
    -- the T2 tactical missile launchers fire at a tactical anti-missile.
    tml = { ue = 'ueb2108', ua = 'uab2108', ur = 'urb2108', xs = 'xsb2108' },
    -- ... and the strategic missile launchers fire at a strategic one.
    nuke = { ue = 'ueb2305', ua = 'uab2305', ur = 'urb2305', xs = 'xsb2305' },
    -- ... and the T1 torpedo launchers (attack order) at an anti-torpedo.
    torpedo = { ue = 'ueb2109', ua = 'uab2109', ur = 'urb2109', xs = 'xsb2109' },
})

-- Per-case policy overrides, first row matching wins.
-- attacker: blueprint id or Lua pattern on it; weapon: 1-based index or nil (any);
-- label: pattern on the weapon's label; air: match air attackers only.
-- Optional fields: target (bp id, or a catalog key like 'building' resolved
-- per the attacker's faction), distance and budget (world units / ticks),
-- unsupported (reason string: report UNSUPPORTED without engagement).
-- A row whose target the weapon's TargetRestrictOnlyAllow / Disallow forbid
-- does not match that weapon.
rawset(_G, 'WEAPON_AUDIT_OVERRIDES', {
    -- Drop weapons carry no FireTargetLayerCapsTable by design: FAF's attack
    -- order drives the release run (our order path bypasses caps the same
    -- way), so give them a real engagement instead of the no-caps verdict.
    { air = true, label = '^Bomb', target = 'building', distance = 16, budget = 1500 },
    { air = true, label = 'Torpedo', target = 'ship', distance = 20, budget = 1500 },
    { air = true, label = 'Missile', target = 'building', distance = 18, budget = 1500 },
})
