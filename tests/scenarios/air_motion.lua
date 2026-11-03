-- Scenario test (tests/scenarios.rs): aircraft vertical motion events
-- (OnMotionVertEventChange): a gunship made on the ground and sent off
-- climbs ('Up') then levels at its flight height ('Top'); FAF's AirUnit
-- handler runs without errors.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local g = CreateUnitHPR('uea0203', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    -- (Scripts' aircraft start at their flying height: put it on the ground.)
    Warp(g, { 400, GetTerrainHeight(400, 600), 600 })
    local seen = {}
    local orig = g.OnMotionVertEventChange
    g.OnMotionVertEventChange = function(self, new, old)
        table.insert(seen, old .. '>' .. new)
        return orig(self, new, old)
    end
    IssueMove({g}, {440, 0, 600})
    WaitTicks(80)
    local s = table.concat(seen, ' ')
    check(seen[1] == 'Bottom>Up', 'it takes off (Bottom>Up): ' .. s)
    check(string.find(s, 'Up>Top') ~= nil, 'and levels at its height (Up>Top): ' .. s)
    ScenarioResult.done = true
end)
