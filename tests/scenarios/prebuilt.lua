-- Scenario test (tests/scenarios.rs): the PrebuiltUnits start. simInit's
-- InitializePrebuiltUnits -> ArmyInitializePrebuiltUnits -> the brain's
-- OnSpawnPreBuiltUnits, which places a factory and four generators near the
-- start (CreateUnitNearSpot) and four extractors on the nearest mass
-- deposits (CreateResourceBuildingNearest), all finished.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    local sx, sz = brain:GetArmyStartPos()
    InitializePrebuiltUnits('ARMY_1')
    check(brain.PreBuilt, 'the brain marks itself prebuilt')
    local units = brain:GetListOfUnits(categories.STRUCTURE, false)
    local count = {}
    for _, u in units do
        count[u.UnitId] = (count[u.UnitId] or 0) + 1
        check(u:GetFractionComplete() == 1, u.UnitId .. ' is finished')
        local p = u:GetPosition()
        check(VDist2(p[1], p[3], sx, sz) < 40, u.UnitId .. ' is near the start')
    end
    check(count.ueb0101 == 1, 'one factory: ' .. tostring(count.ueb0101))
    check(count.ueb1101 == 4, 'four generators: ' .. tostring(count.ueb1101))
    check(count.ueb1103 == 4, 'four extractors: ' .. tostring(count.ueb1103))
    -- No two structures overlap: their centres are at least their combined
    -- half footprints apart on one axis.
    for i = 1, table.getn(units) do
        for j = i + 1, table.getn(units) do
            local a, b = units[i], units[j]
            local pa, pb = a:GetPosition(), b:GetPosition()
            local ha = { a.Blueprint.Footprint.SizeX / 2, a.Blueprint.Footprint.SizeZ / 2 }
            local hb = { b.Blueprint.Footprint.SizeX / 2, b.Blueprint.Footprint.SizeZ / 2 }
            check(math.abs(pa[1] - pb[1]) >= ha[1] + hb[1] or math.abs(pa[3] - pb[3]) >= ha[2] + hb[2], a.UnitId .. ' and ' .. b.UnitId .. ' overlap')
        end
    end
    ScenarioResult.done = true
end)
