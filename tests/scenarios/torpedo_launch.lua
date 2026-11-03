-- Scenario test (tests/scenarios.rs): a `RealisticOrdinance` torpedo from a
-- sub that isn't moving starts at rest and accelerates away.
-- Moho launches it at its launcher's velocity, rescaled to the weapon's
-- muzzle velocity through a normalize that leaves a zero vector zero
-- (MohoEngine.dll 0x102b2745, 0x10051db2); the real game's torpedoes
-- (dev/probes/maps/probe_water) start at 0 and gain 0.3-0.7 a tick each
-- second (`Acceleration` 5 +- `AccelerationRange` 2).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do
            for x = 100, w - 100, 8 do
                if not site and depth(x, z) > 8 and depth(x + 25, z) > 8 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        local shooter = CreateUnitHPR('ues0203', 'ARMY_1', sx, GetSurfaceHeight(sx, sz), sz, 0, 0, 0)
        local target = CreateUnitHPR('ues0203', 'ARMY_2', sx + 25, GetSurfaceHeight(sx + 25, sz), sz, 0, 0, 0)
        target:SetMaxHealth(1000000) target:SetHealth(target, 1000000)
        target:SetFireState(1)
        local speed = function(p) local x, y, z = p:GetVelocity() return math.sqrt(x * x + y * y + z * z) end
        local torpedo, launched
        local weapon = shooter:GetWeaponByLabel('Torpedo01')
        local create = weapon.CreateProjectileAtMuzzle
        weapon.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            if p and not torpedo then
                torpedo, launched = p, speed(p)
            end
            return p
        end
        for _ = 1, 150 do
            if torpedo then break end
            WaitTicks(1)
        end
        check(torpedo ~= nil, 'the sub fired a torpedo')
        if not torpedo then return end
        WaitTicks(10)
        local after = torpedo:BeenDestroyed() and -1 or speed(torpedo)
        note(string.format('torpedo speed at launch %.3f, a second later %.3f (per tick)', launched, after))
        check(launched == 0, 'a torpedo from a still sub starts at rest: ' .. launched)
        check(after >= 0.3 and after <= 0.7, 'it accelerates at 5 +- 2 a second: ' .. after)
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
