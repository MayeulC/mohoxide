-- Scenario test (tests/scenarios.rs): a UEF cruiser's cruise missiles
-- leave their vertical launch tubes straight up and FAF's flight script
-- (TacticalMissileComponent) brings them up and over onto the target. They
-- used to leave along the weapon's aim, skimming the water at the target.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do for x = 100, w - 100, 8 do
            if not site and depth(x, z) > 4 and depth(x + 60, z) > 4 and depth(x + 30, z) > 4 then site = { x, z } end
        end end
        check(site ~= nil, 'found open water')
        if not site then return end
        local sx, sz = site[1], site[2]
        local water = GetSurfaceHeight(sx, sz)
        local c = CreateUnitHPR('ues0202', 'ARMY_1', sx, water, sz, 0, 0, 0)
        local t = CreateUnitHPR('ues0103', 'ARMY_2', sx + 60, GetSurfaceHeight(sx + 60, sz), sz, 0, 0, 0)
        t:SetMaxHealth(100000) t:SetHealth(t, 100000)
        local wpn
        for i = 1, c:GetWeaponCount() do if c:GetWeapon(i).Blueprint.Label == 'CruiseMissile' then wpn = c:GetWeapon(i) end end
        local peak, launched = -1, 0
        local cp = wpn.CreateProjectileAtMuzzle
        wpn.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = cp(self, muzzle)
            if p then
                launched = launched + 1
                ForkThread(function()
                    while not p:BeenDestroyed() do
                        local _, y = p:GetPositionXYZ()
                        peak = math.max(peak, y - water)
                        WaitTicks(1)
                    end
                end)
            end
            return p
        end
        WaitTicks(300)
        local dealt = 100000 - t:GetHealth()
        note(string.format('%d missiles, highest %.1f above the water, %.0f damage', launched, peak, dealt))
        check(launched > 0, 'the cruiser fired its missiles')
        check(peak > 15, string.format('the missiles climb (peak %.1f above the water)', peak))
        check(dealt > 0, 'and come down on the target')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
