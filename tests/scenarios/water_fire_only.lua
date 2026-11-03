-- Scenario test (tests/scenarios.rs): weapons marked `AboveWaterFireOnly`
-- hold fire with their muzzle under the water (a Fatboy walking the seabed
-- doesn't shoot the frigate overhead; on land it does), and
-- `BelowWaterFireOnly` ones fire only submerged (the Cybran ACU's torpedo).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        -- Deep water (the Fatboy is tall).
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do
            for x = 100, w - 100, 8 do
                if not site and depth(x, z) > 12 and depth(x + 15, z) > 12 and depth(x - 15, z) > 12 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        note(string.format('site %d,%d depth %.1f', sx, sz, depth(sx, sz)))
        local function tough(u) u:SetMaxHealth(1000000) u:SetHealth(u, 1000000) return u end
        local function trial(id, x, z, target, tx, tz, ticks, enhancement)
            local u = tough(CreateUnitHPR(id, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0))
            if enhancement then u:CreateEnhancement(enhancement) end
            local t = tough(CreateUnitHPR(target, 'ARMY_2', tx, GetSurfaceHeight(tx, tz), tz, 0, 0, 0))
            -- (Aircraft patrol on the spot: idle, they'd land.)
            if EntityCategoryContains(categories.AIR, t) then IssuePatrol({ t }, { tx, 0, tz + 3 }) end
            WaitTicks(ticks)
            local dealt = 1000000 - t:GetHealth()
            local layer = u:GetCurrentLayer()
            u:Destroy() t:Destroy()
            WaitTicks(2)
            return dealt, layer
        end
        local dealt, layer = trial('uel0401', sx, sz, 'uea0203', sx + 6, sz, 150)
        note(string.format('Fatboy on the seabed (%s) at a gunship overhead: %.0f damage', layer, dealt))
        check(layer == 'Seabed', 'the Fatboy is on the seabed: ' .. layer)
        check(dealt == 0, 'a submerged Fatboy holds fire')
        dealt = trial('uel0401', 400, 600, 'uel0201', 412, 600, 150)
        note(string.format('Fatboy on land at a tank: %.0f damage', dealt))
        check(dealt > 0, 'a Fatboy on land fires')
        dealt = trial('url0001', sx, sz, 'ues0103', sx + 8, sz, 200, 'NaniteTorpedoTube')
        note(string.format('Cybran ACU on the seabed at a frigate overhead: %.0f damage', dealt))
        check(dealt > 0, 'a submerged Cybran ACU fires its torpedo')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
