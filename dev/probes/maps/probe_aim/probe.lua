-- Differential probe: how a turret whose pitch bone is modelled tilted aims
--, logged in a format both engines print identically ("PROBE ..."
-- lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_aim 90
--   ours: cargo run -- --headless --no-ai --armies 8 --ticks 500 --script dev/probes/maps/probe_aim/probe.lua
-- Three pairs, each its own two armies (every other pair allied), each
-- target spotted by a scout of the shooter's army:
-- 1. the Salvation (xab2307; its barrel bone rests 65 degrees up, high arc)
--    at a T3 tank 200 west;
-- 2. the Tempest (uas0401; its barrel bone rests straight up, its surfacing clip lowers it; low arc,
--    TurretPitch 15 +- 35) at a frigate 60 east, on the water;
-- 3. a T1 tank (uel0201, nothing tilted) at a tank 15 east;
-- 4. walkers whose walk clip sways the bone the turret sits on (the Mantis,
--    url0107, its Torso; the Selen, xsl0101, the Torso under its Turret),
--    walking back and forth past a tank 10 to their side.
-- Every 2 ticks for the walkers: the muzzle's heading off the target's
-- bearing and its elevation (a steady barrel on a swaying torso holds both).
-- Every 10 ticks: each shooter's muzzle bone direction (heading from +z
-- toward +x, elevation, degrees), its weapon target, and the target's
-- bearing; each shot: the projectile's launch heading and elevation.
local TICKS = 450
local armies = ListArmies()

local function sweep()
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep()

local function deg(x, y, z)
    return math.deg(math.atan2(x, z)), math.deg(math.atan2(y, math.sqrt(x * x + z * z)))
end

ForkThread(function()
    WaitTicks(2)
    sweep()
    for a = 1, table.getn(armies) do
        for b = 1, table.getn(armies) do
            if a ~= b then SetAlliance(a, b, 'Ally') end
        end
        ArmyBrains[a]:GiveStorage('ENERGY', 5000000) ArmyBrains[a]:GiveResource('ENERGY', 5000000)
    end
    SetAlliance(1, 2, 'Enemy') SetAlliance(3, 4, 'Enemy') SetAlliance(5, 6, 'Enemy') SetAlliance(7, 8, 'Enemy')
    local function at(bp, army, x, z, y)
        return CreateUnitHPR(bp, armies[army], x, y or GetTerrainHeight(x, z), z, 0, 0, 0)
    end
    local pairs = {
        { 'salvation', at('xab2307', 1, 384, 896), at('uel0303', 2, 184, 896), 'Turret_Muzzle' },
        { 'tempest', at('uas0401', 3, 600, 600, GetSurfaceHeight(600, 600)), at('ues0103', 4, 660, 600, GetSurfaceHeight(660, 600)), 'Turret_Muzzle' },
        { 'tank', at('uel0201', 5, 384, 696), at('uel0201', 6, 399, 696), 'Turret_Muzzle' },
    }
    for _, p in pairs do
        local name, shooter, target = p[1], p[2], p[3]
        target:SetMaxHealth(1e7) target:SetHealth(nil, 1e7) target:SetFireState(1)
        local q = target:GetPosition()
        local scout = CreateUnitHPR('uel0101', armies[shooter:GetArmy()], q[1], GetTerrainHeight(q[1], q[3] + 8), q[3] + 8, 0, 0, 0)
        scout:SetFireState(1)
        local x, y, z = shooter:GetBoneDirection(p[4])
        local h, e = deg(x, y, z)
        LOG(string.format('PROBE rest %s heading %.1f elevation %.1f (unit heading %.1f)', name, h, e, math.deg(shooter:GetHeading())))
        local w = shooter:GetWeapon(1)
        local wbp = w:GetBlueprint()
        LOG(string.format('PROBE bp %s pitch %s range %s yaw %s range %s', name, tostring(wbp.TurretPitch), tostring(wbp.TurretPitchRange), tostring(wbp.TurretYaw), tostring(wbp.TurretYawRange)))
        local create = w.CreateProjectileForWeapon
        w.CreateProjectileForWeapon = function(self, bone)
            local proj = create(self, bone)
            if proj then
                local vx, vy, vz = proj:GetVelocity()
                local ph, pe = deg(vx, vy, vz)
                LOG(string.format('PROBE shot %s t=%d heading %.1f elevation %.1f', name, GetGameTick(), ph, pe))
            end
            return proj
        end
    end
    local walkers = {}
    for i, w in { { 'mantis', 'url0107', 790 }, { 'selen', 'xsl0101', 850 } } do
        local u = at(w[2], 7, 300, w[3])
        local t = at('uel0201', 8, 320, w[3] + 10)
        t:SetMaxHealth(1e7) t:SetHealth(nil, 1e7) t:SetFireState(1)
        u:SetMaxHealth(1e7) u:SetHealth(nil, 1e7)
        local scout = CreateUnitHPR('uel0101', armies[7], 320, GetTerrainHeight(320, w[3] + 18), w[3] + 18, 0, 0, 0)
        scout:SetFireState(1)
        for k = 1, 6 do
            local x = (math.mod(k, 2) == 1) and 340 or 300
            IssueMove({ u }, { x, GetTerrainHeight(x, w[3]), w[3] })
        end
        table.insert(walkers, { w[1], u, t })
    end
    ForkThread(function()
        WaitTicks(20)
        for _ = 1, 100 do
            WaitTicks(2)
            for _, w in walkers do
                local name, u, t = w[1], w[2], w[3]
                if not u:BeenDestroyed() and not t:BeenDestroyed() then
                    local x, y, z = u:GetBoneDirection(name == 'mantis' and 'Turret_Muzzle_01' or 'Turret_Muzzle')
                    local h, e = deg(x, y, z)
                    local a, b = u:GetPosition(), t:GetPosition()
                    local bearing = math.deg(math.atan2(b[1] - a[1], b[3] - a[3]))
                    local off = h - bearing
                    if off > 180 then off = off - 360 elseif off < -180 then off = off + 360 end
                    LOG(string.format('PROBE walk %s t=%d off %.1f elevation %.1f', name, GetGameTick(), off, e))
                end
            end
        end
    end)
    WaitTicks(5)
    for _, p in pairs do IssueAttack({ p[2] }, p[3]) end
    for t = 1, TICKS / 10 do
        WaitTicks(10)
        for _, p in pairs do
            local name, u, target = p[1], p[2], p[3]
            if not u:BeenDestroyed() and not target:BeenDestroyed() then
                local x, y, z = u:GetBoneDirection(p[4])
                local h, e = deg(x, y, z)
                local a, b = u:GetPosition(), target:GetPosition()
                local bearing = math.deg(math.atan2(b[1] - a[1], b[3] - a[3]))
                local w = u:GetWeapon(1)
                local wt = w:GetCurrentTarget()
                local ah, ap = 0, 0
                if w.AimControl then ah, ap = w.AimControl:GetHeadingPitch() end
                LOG(string.format('PROBE aim %s t=%d muzzle heading %.1f elevation %.1f target %s bearing %.1f manipulator heading %.1f pitch %.1f',
                    name, GetGameTick(), h, e, wt and 'yes' or 'no', bearing, math.deg(ah), math.deg(ap)))
                if name == 'tank' then
                    -- The hull's roll: its exhausts sit level across it.
                    local l, r = u:GetPosition('Exhaust_Left'), u:GetPosition('Exhaust_Right')
                    LOG(string.format('PROBE hull tank t=%d roll %.1f', GetGameTick(), math.deg(math.atan2(l[2] - r[2], VDist2(l[1], l[3], r[1], r[3])))))
                end
            end
        end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
