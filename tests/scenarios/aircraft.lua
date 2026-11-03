-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    return b
end
-- Aircraft fly: an interceptor reaches the point, then (idle) slows onto it
-- and lands nearby, as the real game's fighters do (dev/probes probe_air); a
-- gunship flies there and hovers.
ForkThread(function()
    WaitTicks(2)
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 380, 40, 600, 0, 0, 0)
    local gunship = CreateUnitHPR('uea0203', 'ARMY_1', 380, 40, 610, 0, 0, 0)
    IssueMove({plane}, {460, 0, 600})
    IssueMove({gunship}, {460, 0, 610})
    WaitTicks(150)
    local p, g = plane:GetPosition(), gunship:GetPosition()
    check(VDist2(p[1], p[3], 460, 600) < 25, 'the interceptor got there')
    check(VDist2(g[1], g[3], 460, 610) < 1.5, 'the gunship hovers on the point')
    for _ = 1, 400 do
        if plane:GetCurrentLayer() ~= 'Air' then break end
        WaitTicks(1)
    end
    local p3 = plane:GetPosition()
    note(string.format('interceptor landed at %.1f %.1f, %.2f up', p3[1], p3[3], p3[2] - GetTerrainHeight(p3[1], p3[3])))
    check(plane:GetCurrentLayer() == 'Land', 'then lands, idle')
    check(VDist2(p3[1], p3[3], 460, 600) < 10, 'by the point')
    check(p3[2] - GetTerrainHeight(p3[1], p3[3]) < 1, 'on the ground')
    done()
end)
