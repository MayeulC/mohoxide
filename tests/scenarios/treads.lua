-- Scenario test (tests/scenarios.rs): a UEF T1 tank driving off gets FAF's
-- tread scroller (TreadComponent: AddThreadScroller(1, ScrollMultiplier));
-- the test reads its scroll values from the engine.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
ForkThread(function()
    WaitTicks(2)
    local t = CreateUnitHPR('uel0201', 'ARMY_1', 650, GetTerrainHeight(650, 320), 320, 0, math.pi / 2, 0)
    IssueMove({t}, {690, 0, 320})
    WaitTicks(40)
    -- An interceptor's panels ping-pong (AirUnit.AddPingPong, 8 numbers).
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 660, GetTerrainHeight(660, 330) + 20, 330, 0, 0, 0)
    rawset(_G, 'PingPongPlane', plane:GetEntityId())
    rawset(_G, 'TreadTank', t:GetEntityId())
    ScenarioResult.done = true
end)
