-- Scenario test (tests/scenarios.rs): navigator:GetGoalPos(), which Uveso's
-- ACU behaviour compares with the ACU's position: idle, where it stands;
-- moving, the order's goal; with SetGoal and no order, that point.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function near(a, b) return VDist2(a[1], a[3], b[1], b[3]) < 0.5 end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local nav = tank:GetNavigator()
    check(near(nav:GetGoalPos(), tank:GetPosition()), 'idle: where it stands')
    IssueMove({tank}, {440, 0, 620})
    WaitTicks(2)
    check(near(nav:GetGoalPos(), {440, 0, 620}), 'moving: the order\'s goal')
    IssueClearCommands({tank})
    nav:SetGoal({380, 0, 590})
    WaitTicks(1)
    check(near(nav:GetGoalPos(), {380, 0, 590}), 'SetGoal: that point')
    ScenarioResult.done = true
end)
