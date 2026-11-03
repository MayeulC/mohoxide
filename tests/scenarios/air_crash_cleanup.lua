-- Scenario test (tests/scenarios.rs): aircraft killed in flight crash and are
-- removed (FAF's crash: OnImpact on the ground, then the death thread), a
-- few of each faction's scouts and fighters.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local planes = {}
        for i, bp in { 'xsa0101', 'uea0101', 'ura0101', 'uaa0101', 'xsa0102', 'uea0102' } do
            local x = 380 + i * 10
            local p = CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, 600) + 20, 600, 0, 0, 0)
            IssueMove({ p }, { x, 0, 700 })
            table.insert(planes, p)
        end
        WaitTicks(40)
        for _, p in planes do p:Kill() end
        WaitTicks(400)
        for _, p in planes do
            local where = IsDestroyed(p) and 'removed' or string.format('still there, %.1f above the ground, layer %s', p:GetPosition()[2] - GetTerrainHeight(p:GetPosition()[1], p:GetPosition()[3]), p.Layer or '?')
            note(string.format('%s: %s', p.UnitId, where))
            check(IsDestroyed(p), p.UnitId .. ' was removed after crashing')
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
