-- Scenario test (tests/scenarios.rs): weapons marked `AboveWaterTargetsOnly`
-- engage a unit on the seabed only when one of its target points
-- (`AI.TargetBones`, else its position) is above the water (MohoEngine.dll
-- 0x102b146f): a frigate's gun ignores an amphibious tank under the waves,
-- shoots another ship, and shoots a Galactic Colossus wading with its torso
-- out of the water.
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
                if not site and depth(x, z) > 8 and depth(x + 15, z) > 8 and depth(x - 15, z) > 8 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        note(string.format('site %d,%d depth %.1f', sx, sz, depth(sx, sz)))
        local function tough(u) u:SetMaxHealth(1000000) u:SetHealth(u, 1000000) return u end
        -- The frigate's main gun only: its AA can't reach the seabed anyway.
        local function trial(target, x, z, tx, tz, ticks, floats)
            local f = tough(CreateUnitHPR('ues0103', 'ARMY_1', x, GetSurfaceHeight(x, z), z, 0, 0, 0))
            local t = tough(CreateUnitHPR(target, 'ARMY_2', tx, floats and GetSurfaceHeight(tx, tz) or GetTerrainHeight(tx, tz), tz, 0, 0, 0))
            t:SetCanTakeDamage(true)
            for i = 1, t:GetWeaponCount() do t:GetWeapon(i):SetEnabled(false) end
            WaitTicks(ticks)
            local dealt, layer = 1000000 - t:GetHealth(), t:GetCurrentLayer()
            local above = false
            for _, bone in t:GetBlueprint().AI.TargetBones or {} do
                if t:IsValidBone(bone) and t:GetPosition(bone)[2] > GetSurfaceHeight(tx, tz) then above = true end
            end
            f:Destroy() t:Destroy()
            WaitTicks(2)
            return dealt, layer, above
        end
        local dealt, layer = trial('url0203', sx, sz, sx + 10, sz, 100)
        note(string.format('frigate at an amphibious tank (%s): %.0f damage', layer, dealt))
        check(layer == 'Seabed', 'the tank is on the seabed: ' .. layer)
        check(dealt == 0, 'the gun ignores a unit under the water')
        dealt = trial('ues0103', sx, sz, sx + 10, sz, 100, true)
        note(string.format('frigate at a frigate: %.0f damage', dealt))
        check(dealt > 0, 'the gun shoots a ship')
        -- Shallower water, so the Colossus's torso can be above it; the
        -- frigate floats in deeper water next to it.
        local shallow
        for z = 100, h - 100, 4 do
            for x = 100, w - 100, 4 do
                if not shallow and depth(x, z) > 3 and depth(x, z) < 6 and depth(x + 14, z) > 4 then shallow = { x, z } end
            end
        end
        check(shallow ~= nil, 'found shallow water')
        if not shallow then return end
        local above
        dealt, layer, above = trial('ual0401', shallow[1] + 14, shallow[2], shallow[1], shallow[2], 150)
        note(string.format('frigate at a Galactic Colossus (%s, a target bone above water: %s): %.0f damage', layer, tostring(above), dealt))
        check((dealt > 0) == above, 'it shoots the Colossus exactly when a target bone is above water')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
