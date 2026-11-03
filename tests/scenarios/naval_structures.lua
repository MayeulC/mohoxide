-- Scenario test (tests/scenarios.rs): structures built on water float or
-- sink by their blueprint's `BuildOnLayerCaps`. A naval factory
-- (LAYER_Water) is built on the water surface, in the Water layer; mass
-- storage (LAYER_Seabed only) sits on the seabed, in the Seabed layer.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local b = ArmyBrains[1]
        b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
        b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        -- Open water at least 3 deep with land 14 to the east.
        local site, shore
        local w, h = GetMapSize()
        for z = 200, h - 200, 8 do
            for x = 200, w - 200, 8 do
                if not site and depth(x, z) > 3 and depth(x + 14, z) <= 0 and depth(x - 8, z) > 3 and depth(x, z + 8) > 3 and depth(x, z - 8) > 3 then
                    site, shore = { x + 0.5, z + 0.5 }, { x + 16, z }
                end
            end
        end
        check(site ~= nil, 'found deep water by a shore')
        if not site then return end
        local sx, sz = site[1], site[2]
        note(string.format('site %.1f,%.1f: seabed %.2f, water %.2f', sx, sz, GetTerrainHeight(sx, sz), GetSurfaceHeight(sx, sz)))
        local eng = CreateUnitHPR('uel0105', 'ARMY_1', shore[1], GetTerrainHeight(shore[1], shore[2]), shore[2], 0, 0, 0)
        IssueBuildMobile({ eng }, { sx, GetSurfaceHeight(sx, sz), sz }, 'ueb0103', {})
        local factory
        for _ = 1, 600 do
            WaitTicks(1)
            local f = b:GetListOfUnits(categories.ueb0103, false) or {}
            if f[1] then factory = f[1] break end
        end
        if not factory then
            local x, _, z = eng:GetPositionXYZ()
            note(string.format('not started: engineer at %.1f,%.1f, %d orders, layer %s', x, z, table.getn(eng:GetCommandQueue()), eng:GetCurrentLayer()))
        end
        check(factory ~= nil, 'the naval factory was started')
        if factory then
            local _, y = factory:GetPositionXYZ()
            note(string.format('naval factory at y %.2f, layer %s', y, factory:GetCurrentLayer()))
            check(math.abs(y - GetSurfaceHeight(sx, sz)) < 0.01, string.format('the naval factory floats: y %.2f, water %.2f', y, GetSurfaceHeight(sx, sz)))
            check(factory:GetCurrentLayer() == 'Water', 'the naval factory is in the Water layer: ' .. factory:GetCurrentLayer())
        end
        local storage = CreateUnitHPR('ueb1106', 'ARMY_1', sx - 8, GetTerrainHeight(sx - 8, sz), sz, 0, 0, 0)
        check(storage:GetCurrentLayer() == 'Seabed', 'mass storage under water is in the Seabed layer: ' .. storage:GetCurrentLayer())
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
