-- Differential probe (weapon aim rework, phase 4): when a new unit's weapon
-- first takes a target, and what a turret does while its weapon or its aim
-- is disabled. Logged in a format both engines print identically.
--   real game (Windows): dev/probes/run_faf.ps1 probe_aimtiming 40
--   ours: cargo run -- --headless --no-ai --ticks 200 --script dev/probes/maps/probe_aimtiming/probe.lua
-- Every unit is invulnerable and faces +z; each enemy stands 12 to its +x
-- (90 degrees off its facing). Every tick: the game tick, each shooter's
-- target ('-' none), aim heading and pitch, the yaw bone's heading in the
-- world (the pose: GetHeadingPitch reports the aim's stored angles), shots
-- so far, and whether army 1 sees its enemy now ('s' seen, '-' not).
-- A. A UEF T1 tank (uel0201): at tick 60 `SetWeaponEnabled(false)`, at
--    tick 100 enabled again. Its enemy moves 10 north from tick 65.
-- B. The same, but its aim controller `Disable()`d at 60 and `Enable()`d
--    at 100. Its enemy moves too.
-- C. A UEF T1 point defence (ueb2101), never disabled (a structure's
--    acquisition timing).
local TICKS = 140
local armies = ListArmies()
local function f2(n) return string.format('%.2f', n) end

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function spawn(bp, army, x, z)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    u:SetCanTakeDamage(false)
    return u
end

-- Shots per weapon (CreateProjectileAtMuzzle).
local function counter(w)
    local shots = { n = 0 }
    local create = w.CreateProjectileAtMuzzle
    w.CreateProjectileAtMuzzle = function(self, muzzle)
        shots.n = shots.n + 1
        return create(self, muzzle)
    end
    return shots
end

local shooters = {}
for _, s in {{'A', 'uel0201', 420, 580}, {'B', 'uel0201', 650, 190}, {'C', 'ueb2101', 650, 160}} do
    local name, bp, x, z = s[1], s[2], s[3], s[4]
    local u = spawn(bp, armies[1], x, z)
    IssueStop({u})
    local enemy = spawn('uel0201', armies[2], x + 12, z)
    IssueStop({enemy})
    local w = u:GetWeapon(1)
    table.insert(shooters, { name = name, unit = u, weapon = w, shots = counter(w), enemy = enemy, x = x, z = z })
end

local function line(s)
    local w = s.weapon
    local t = w:GetCurrentTarget()
    local m = s.unit:GetWeaponManipulatorByLabel(w.Blueprint.Label)
    local h, p = 0, 0
    if m then h, p = m:GetHeadingPitch() end
    local bx, by, bz = s.unit:GetBoneDirection(w.Blueprint.TurretBoneYaw)
    local blip = s.enemy:GetBlip(1)
    local seen = blip and blip:IsSeenNow(1)
    LOG('PROBE ', GetGameTick(), ' ', s.name, ' target ', t and 't' or '-', ' heading ', f2(h), ' pitch ', f2(p), ' bone ', f2(math.atan2(bx, bz)), ' shots ', s.shots.n, ' seen ', seen and 's' or '-')
end

ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    for tick = 1, TICKS do
        if tick == 60 then
            shooters[1].weapon:SetWeaponEnabled(false)
            shooters[2].weapon.AimControl:Disable()
            LOG('PROBE ', GetGameTick(), ' disabled')
        elseif tick == 65 then
            for i = 1, 2 do
                local s = shooters[i]
                IssueMove({s.enemy}, {s.x + 12, GetTerrainHeight(s.x + 12, s.z + 10), s.z + 10})
            end
        elseif tick == 100 then
            shooters[1].weapon:SetWeaponEnabled(true)
            shooters[2].weapon.AimControl:Enable()
            LOG('PROBE ', GetGameTick(), ' enabled')
        end
        for _, s in shooters do line(s) end
        WaitTicks(1)
    end
    LOG('PROBE done')
end)
