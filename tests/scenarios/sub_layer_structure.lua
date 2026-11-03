-- Scenario test (tests/scenarios.rs): A structure built only in
-- the Sub layer (the HARMS, xrb2309: BuildOnLayerCaps LAYER_Sub alone) is
-- in Sub when made at the water surface, and stays there as it sinks; the
-- real game's does (dev/probes/maps/probe_auditgaps, lane g2). A naval
-- factory (LAYER_Water) still floats.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local sites = {}
        local w, h = GetMapSize()
        for z = 100, h - 100, 16 do
            for x = 100, w - 100, 16 do
                if table.getn(sites) < 2 and depth(x, z) > 10 and (not sites[1] or VDist2(x, z, sites[1][1], sites[1][2]) > 60) then
                    table.insert(sites, { x + 0.5, z + 0.5 })
                end
            end
        end
        check(table.getn(sites) == 2, 'found deep water')
        if table.getn(sites) < 2 then return end
        local s = sites[1]
        local harms = CreateUnitHPR('xrb2309', 'ARMY_1', s[1], GetSurfaceHeight(s[1], s[2]), s[2], 0, 0, 0)
        check(harms:GetCurrentLayer() == 'Sub', 'HARMS made at the surface is in Sub: ' .. harms:GetCurrentLayer())
        local f = sites[2]
        local factory = CreateUnitHPR('ueb0103', 'ARMY_1', f[1], GetSurfaceHeight(f[1], f[2]), f[2], 0, 0, 0)
        check(factory:GetCurrentLayer() == 'Water', 'naval factory floats: ' .. factory:GetCurrentLayer())
        WaitTicks(60)
        local y = harms:GetPosition()[2]
        check(y < GetSurfaceHeight(s[1], s[2]) - 2, 'HARMS sank: y ' .. y)
        check(harms:GetCurrentLayer() == 'Sub', 'HARMS still in Sub after sinking: ' .. harms:GetCurrentLayer())
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
