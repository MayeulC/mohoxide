-- Differential probe: which dummy weapons call OnFire (the Fire
-- Beetle's ExplosionRadiusRingDummyWeapon fires every reload in ours).
--   real game (Windows): request 'probe_dummies 60' to the watcher
--   ours: cargo run -- --headless --no-ai --no-audio --ticks 110 --script dev/probes/maps/probe_dummies/probe.lua
-- Armies: tested units in army 1, targets in NEUTRAL_CIVILIAN (made its
-- enemy, on hold fire, 30000 hp).
-- Lanes (unit, its target's distance, order):
--   ringatk:  xrl0302, target 4, IssueAttack (Suicide kept off: the
--             probe ignores the script's SetEnabled(true), so it never
--             kills itself); ringidle: the same, no order; ringfar: the
--             same with IssueAttack on a target 12 away.
--   uel0104 / uel0307 / url0101: mobile AA / shield / scout, Damage 0 target
--             trackers with TargetPriorities, no order, target 10.
--   xsl0305:  DummyWeapon with RateOfFire 0, target 15.
--   url0001:  Cybran ACU DummyWeapon (Damage 0, turreted), target 15.
--   tank:     uel0201 control, target 10.
--   ringon / ringonfar / ringonidle: the ring lanes with the Suicide kept on
--             instead, its OnFire not passed on (so it never kills).
-- Per weapon: OnGotTarget, OnFire and CreateProjectileAtMuzzle, each logged
-- the first 4 times with the tick, then counts at the end.
-- Lines: PROBE <tick> <tag> <event> ...
local fmt = string.format
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local armies = ListArmies()
local TESTER = armies[1]
local ENEMY = 'NEUTRAL_CIVILIAN'
SetAlliance(TESTER, ENEMY, 'Enemy')
SetAlliance(ENEMY, TESTER, 'Enemy')

local function P(tag, s, a, b, c, d)
    LOG(fmt('PROBE %d %s ', GetGameTick(), tag) .. fmt(s, a, b, c, d))
end

-- Flat land on SCMP_009 (from probe_auditgaps).
local LAND = {
    { 608, 80 }, { 688, 80 }, { 912, 96 }, { 784, 144 },
    { 656, 160 }, { 736, 224 }, { 816, 224 }, { 912, 240 },
    { 672, 304 }, { 800, 336 }, { 960, 336 }, { 656, 384 },
}

local function make(bp, army, x, z)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
end

local lanes = {}
local function lane(tag, bp, site, dist, opts)
    opts = opts or {}
    local s = LAND[site]
    local target = make('uel0201', ENEMY, s[1], s[2] + dist)
    target:SetFireState(1)
    target:SetMaxHealth(30000) target:SetHealth(nil, 30000)
    local u = make(bp, TESTER, s[1], s[2])
    local l = { tag = tag, u = u, weps = {} }
    for i = 1, u:GetWeaponCount() do
        local wep = u:GetWeapon(i)
        local bpw = wep:GetBlueprint()
        local label = bpw.Label or ('weapon' .. i)
        local c = { label = label, got = 0, fire = 0, shot = 0 }
        table.insert(l.weps, c)
        P(tag, 'weapon %d %s rof %s prio %s', i, label, tostring(bpw.RateOfFire), tostring(bpw.TargetPriorities ~= nil))
        local function note(kind)
            c[kind] = c[kind] + 1
            if c[kind] <= 4 then P(tag, '%s %s #%d', kind, label, c[kind]) end
        end
        local onFire = wep.OnFire
        local defused = opts.defuse and label == opts.defuse
        wep.OnFire = function(self)
            note('fire')
            if defused then return end
            return onFire(self)
        end
        local got = wep.OnGotTarget
        wep.OnGotTarget = function(self)
            note('got')
            return got(self)
        end
        local create = wep.CreateProjectileAtMuzzle
        if create then
            wep.CreateProjectileAtMuzzle = function(self, muzzle)
                note('shot')
                return create(self, muzzle)
            end
        end
        if opts.block and label == opts.block then
            local setEnabled = wep.SetEnabled
            wep.SetEnabled = function(self, on) end
            setEnabled(wep, false)
        end
        if opts.force and label == opts.force then
            local setEnabled = wep.SetEnabled
            wep.SetEnabled = function(self, on) end
            setEnabled(wep, true)
        end
    end
    if opts.attack then
        IssueAttack({ u }, target)
        P(tag, 'attack issued')
    end
    table.insert(lanes, l)
end

lane('ringatk', 'xrl0302', 1, 4, { attack = true, block = 'Suicide' })
lane('ringidle', 'xrl0302', 2, 4, { block = 'Suicide' })
lane('ringfar', 'xrl0302', 3, 12, { attack = true, block = 'Suicide' })
lane('uel0104', 'uel0104', 4, 10)
lane('uel0307', 'uel0307', 5, 10)
lane('url0101', 'url0101', 6, 10)
lane('xsl0305', 'xsl0305', 7, 15)
lane('url0001', 'url0001', 8, 15)
lane('tank', 'uel0201', 9, 10)
lane('ringon', 'xrl0302', 10, 4, { attack = true, force = 'Suicide', defuse = 'Suicide' })
lane('ringonfar', 'xrl0302', 11, 12, { attack = true, force = 'Suicide', defuse = 'Suicide' })
lane('ringonidle', 'xrl0302', 12, 4, { force = 'Suicide', defuse = 'Suicide' })

ForkThread(function()
    WaitTicks(100)
    for _, l in lanes do
        for _, c in l.weps do
            P(l.tag, 'total %s got %d fire %d shot %d', c.label, c.got, c.fire, c.shot)
        end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
