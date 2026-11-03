-- Scenario test (tests/scenarios.rs): an air staging platform killed with an
-- aircraft docked lets it go (AirStagingPlatformUnit.Kill calls
-- TransportDetachAllUnits).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local pad = CreateUnitHPR('ueb5202', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 420, GetTerrainHeight(420, 600) + 20, 600, 0, 0, 0)
    WaitTicks(5)
    IssueTransportLoad({ plane }, pad)
    for _ = 1, 300 do
        WaitTicks(1)
        if plane:IsUnitState('Attached') then break end
    end
    check(plane:IsUnitState('Attached'), 'the plane docked')
    local detached = 0
    local onDetach = pad.OnTransportDetach
    pad.OnTransportDetach = function(self, bone, u) detached = detached + 1 return onDetach(self, bone, u) end
    local ok, err = pcall(pad.Kill, pad)
    check(ok, 'Kill runs: ' .. tostring(err))
    if not ok then WARN('staging death scenario: ' .. tostring(err)) end
    check(detached == 1, 'OnTransportDetach once: ' .. detached)
    check(not plane:IsUnitState('Attached'), 'the plane is free')
    WaitTicks(10)
    check(not plane.Dead, 'and alive')
    ScenarioResult.done = true
end)
