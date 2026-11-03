-- Differential probe: anti-torpedo weapons against a torpedo launcher.
-- Logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_antitorp 90
--   ours: cargo run -- --headless --no-ai --ticks 420 --script dev/probes/maps/probe_antitorp/probe.lua
-- Each case at its own deep-water site on SCMP_009: the defender (army 1,
-- 1e7 hp) at the site, the faction's T1 torpedo launcher (NEUTRAL_CIVILIAN,
-- made the enemy, 1e7 hp, vision and sonar 60) 40 west; the launcher attacks on its own for 350
-- ticks (an attack order from its 5th). The dived sub's launcher appears once the sub is down.
-- Lines: PROBE <tick> <case> shot <label> (a projectile left a weapon),
-- 'torp' (the launcher fired), 'deflect' (a torpedo was given a new target,
-- FAF's DepthCharge), 'tdmg' (a torpedo took damage), 'hit <amount>' (the
-- defender took damage), 'flare <n> <x> <y> <z>' (the first two anti-torpedo
-- shots' positions, relative to the defender, every 3 ticks), 'layer'.
-- At the end: PROBE end <case> shots <n> torps <n> deflected <n> damaged <n>
-- hits <n> dmg <total>.
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local fmt = string.format
local TESTER = armies[1]
local ENEMY = 'NEUTRAL_CIVILIAN'
SetAlliance(TESTER, ENEMY, 'Enemy')
SetAlliance(ENEMY, TESTER, 'Enemy')

-- Deep water (over 12) from 50 west to 10 east of each site.
local SITES = {
    { 56, 40 }, { 216, 40 }, { 264, 152 }, { 280, 296 }, { 456, 328 },
    { 632, 616 }, { 792, 664 }, { 840, 776 }, { 712, 856 }, { 808, 920 },
}
-- { tag, defender, launcher, how }
local CASES = {
    { 'ues0201', 'ues0201', 'ueb2109', 'surface' },
    { 'xes0102', 'xes0102', 'ueb2109', 'surface' },
    { 'xes0307', 'xes0307', 'ueb2109', 'surface' },
    { 'xrs0204sub', 'xrs0204', 'urb2109', 'dive' },
    { 'xrs0204', 'xrs0204', 'urb2109', 'surface' },
    { 'xrl0403', 'xrl0403', 'urb2109', 'seabed' },
    { 'xsb2205', 'xsb2205', 'xsb2109', 'surface' },
    { 'xrs0205', 'xrs0205', 'urb2109', 'surface' },
    { 'xss0201', 'xss0201', 'xsb2109', 'surface' },
}
local RUN = 350

local function tough(u)
    u:SetMaxHealth(10000000) u:SetHealth(nil, 10000000)
end
local function isDefence(wbp)
    return wbp.TargetType == 'RULEWTT_Projectile' and wbp.TargetRestrictOnlyAllow == 'TORPEDO'
end

local results = {}
local function startLauncher(r)
    local l = CreateUnitHPR(r.launcherBp, ENEMY, r.x - 40, GetSurfaceHeight(r.x - 40, r.z), r.z, 0, 0, 0)
    tough(l)
    -- sight of the defender (a launcher's own vision doesn't reach 40)
    l:SetIntelRadius('Vision', 60)
    l:SetIntelRadius('Sonar', 60)
    r.launcher = l
    r.started = GetGameTick()
    local tw = l:GetWeapon(1)
    local create = tw.CreateProjectileAtMuzzle
    tw.CreateProjectileAtMuzzle = function(self, muzzle)
        local p = create(self, muzzle)
        r.torps = r.torps + 1
        if r.torps <= 3 then LOG(fmt('PROBE %d %s torp', GetGameTick(), r.tag)) end
        if type(p) == 'table' then
            local snt = p.SetNewTarget
            p.SetNewTarget = function(pp, a1)
                if not pp.seenDeflect then
                    pp.seenDeflect = true
                    r.deflected = r.deflected + 1
                    LOG(fmt('PROBE %d %s deflect', GetGameTick(), r.tag))
                end
                return snt(pp, a1)
            end
            local od = p.OnDamage
            p.OnDamage = function(pp, a1, a2, a3, a4)
                if not pp.seenDamage then
                    pp.seenDamage = true
                    r.damaged = r.damaged + 1
                    LOG(fmt('PROBE %d %s tdmg', GetGameTick(), r.tag))
                end
                if od then return od(pp, a1, a2, a3, a4) end
            end
        end
        return p
    end
end

for i, c in CASES do
    local s = SITES[i]
    local r = { tag = c[1], launcherBp = c[3], how = c[4], x = s[1], z = s[2], shots = 0, torps = 0, deflected = 0, damaged = 0, hits = 0, dmg = 0, flares = {} }
    table.insert(results, r)
    local y = c[4] == 'seabed' and GetTerrainHeight(r.x, r.z) or GetSurfaceHeight(r.x, r.z)
    local u = CreateUnitHPR(c[2], TESTER, r.x, y, r.z, 0, 0, 0)
    tough(u)
    r.u = u
    local on = u.OnDamage
    u.OnDamage = function(self, inst, amount, vec, kind)
        on(self, inst, amount, vec, kind)
        r.hits = r.hits + 1
        r.dmg = r.dmg + amount
        if r.hits <= 3 then LOG(fmt('PROBE %d %s hit %.1f %s', GetGameTick(), r.tag, amount, tostring(kind))) end
    end
    for wi = 1, u:GetWeaponCount() do
        local wep = u:GetWeapon(wi)
        local wbp = wep:GetBlueprint()
        local label = wbp.Label
        local defence = isDefence(wbp)
        local create = wep.CreateProjectileAtMuzzle
        if create then
            wep.CreateProjectileAtMuzzle = function(self, muzzle)
                local p = create(self, muzzle)
                if defence then
                    r.shots = r.shots + 1
                    if r.shots <= 6 then LOG(fmt('PROBE %d %s shot %s', GetGameTick(), r.tag, label)) end
                    if type(p) == 'table' and table.getn(r.flares) < 2 then
                        table.insert(r.flares, { p = p, n = table.getn(r.flares) + 1, t0 = GetGameTick() })
                    end
                end
                return p
            end
        end
    end
    if c[4] == 'dive' then
        IssueDive({ u })
    else
        startLauncher(r)
    end
end

LOG('PROBE start ', GetGameTick())
ForkThread(function()
    for t = 1, RUN + 60 do
        WaitTicks(1)
        local tick = GetGameTick()
        for _, r in results do
            if not r.launcher and r.u:GetCurrentLayer() == 'Sub' then
                LOG(fmt('PROBE %d %s layer Sub', tick, r.tag))
                startLauncher(r)
            end
            for _, f in r.flares do
                local dt = tick - f.t0
                if dt <= 60 and math.mod(dt, 3) == 0 and not f.p:BeenDestroyed() then
                    local x, y, z = f.p:GetPositionXYZ()
                    LOG(fmt('PROBE %d %s flare %d %.2f %.2f %.2f', tick, r.tag, f.n, x - r.x, y - GetSurfaceHeight(x, z), z - r.z))
                end
            end
            if r.started and tick - r.started == 5 then
                IssueAttack({ r.launcher }, r.u)
            end
            if r.started and not r.done and tick - r.started >= RUN then
                r.done = true
                LOG(fmt('PROBE end %s shots %d torps %d deflected %d damaged %d hits %d dmg %.0f', r.tag, r.shots, r.torps, r.deflected, r.damaged, r.hits, r.dmg))
            end
        end
    end
    LOG('PROBE done')
end)
