-- Differential probe: how the Corsair's (dra0202) anti-air darts
-- (CAANanoDart04) fly at an air target. Logged in a format both engines
-- print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_darts 60
--   ours: cargo run -- --headless --no-ai --ticks 260 --script dev/probes/maps/probe_darts/probe.lua
-- Two sites, a Corsair in army 1 and an air target in NEUTRAL_CIVILIAN
-- (made its enemy, hold fire, can't be damaged) 25 east of it, both created
-- 20 above the ground; IssueAttack at tick 10.
--   'A': target ura0101 (scout); 'B': target uea0102 (interceptor).
-- Lines: PROBE <tick> <site> u <corsair x,y,z> t <target x,y,z>;
--        PROBE <tick> <site>d<n> new <x,y,z> v <vel x,y,z per second> tp <target pos>;
--        PROBE <tick> <site>d<n> <x,y,z> v <vel per second> s <speed per second> g <height above ground>;
--        PROBE <tick> <site>d<n> impact <type> <x,y,z> g <height above ground>.
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

local TRACK = 6 -- darts per site logged every tick
local function p3(x, y, z) return fmt('%.2f,%.2f,%.2f', x, y, z) end

local sites = {}
local function site(tag, x, z, target)
    local u = CreateUnitHPR('dra0202', TESTER, x, GetTerrainHeight(x, z) + 20, z, 0, 0, 0)
    local t = CreateUnitHPR(target, ENEMY, x + 25, GetTerrainHeight(x + 25, z) + 20, z, 0, 0, 0)
    t:SetCanTakeDamage(false)
    t:SetFireState(1)
    u:SetCanTakeDamage(false)
    local s = { tag = tag, u = u, t = t, darts = {} }
    table.insert(sites, s)
    local w = u:GetWeaponByLabel('AntiAirMissiles')
    local create = w.CreateProjectileAtMuzzle
    w.CreateProjectileAtMuzzle = function(self, muzzle)
        local p = create(self, muzzle)
        if p then
            local n = table.getn(s.darts) + 1
            local name = s.tag .. 'd' .. n
            table.insert(s.darts, p)
            local x, y, z = p:GetPositionXYZ()
            local vx, vy, vz = p:GetVelocity()
            local tp = p:GetCurrentTargetPosition()
            LOG(fmt('PROBE %d %s new %s v %s tp %s', GetGameTick(), name, p3(x, y, z), p3(10 * vx, 10 * vy, 10 * vz),
                tp and p3(tp[1], tp[2], tp[3]) or 'nil'))
            local impact = p.OnImpact
            p.OnImpact = function(self, kind, other)
                local ix, iy, iz = self:GetPositionXYZ()
                LOG(fmt('PROBE %d %s impact %s %s g %.2f', GetGameTick(), name, tostring(kind), p3(ix, iy, iz), iy - GetTerrainHeight(ix, iz)))
                return impact(self, kind, other)
            end
        end
        return p
    end
end
site('A', 400.5, 400.5, 'ura0101')
site('B', 600.5, 400.5, 'uea0102')

ForkThread(function()
    local start = GetGameTick()
    for t = 0, 240 do
        if t == 10 then
            for _, s in sites do IssueAttack({ s.u }, s.t) end
        end
        local tick = GetGameTick()
        for _, s in sites do
            if not s.u:BeenDestroyed() and not s.t:BeenDestroyed() then
                local ux, uy, uz = s.u:GetPositionXYZ()
                local tx, ty, tz = s.t:GetPositionXYZ()
                LOG(fmt('PROBE %d %s u %s t %s', tick, s.tag, p3(ux, uy, uz), p3(tx, ty, tz)))
            end
            for n, p in s.darts do
                if n <= TRACK and not p:BeenDestroyed() then
                    local x, y, z = p:GetPositionXYZ()
                    local vx, vy, vz = p:GetVelocity()
                    LOG(fmt('PROBE %d %sd%d %s v %s s %.2f g %.2f', tick, s.tag, n, p3(x, y, z), p3(10 * vx, 10 * vy, 10 * vz),
                        10 * p:GetCurrentSpeed(), y - GetTerrainHeight(x, z)))
                end
            end
        end
        WaitTicks(1)
    end
    for _, s in sites do LOG(fmt('PROBE %s fired %d', s.tag, table.getn(s.darts))) end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
