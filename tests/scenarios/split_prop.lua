-- Scenario test (tests/scenarios.rs): a tree group whose blueprint names a
-- SingleTreeBlueprint breaks up through SplitProp (proptree.lua:318): one
-- tree per bone but the root, and the group is gone.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local x, z = 420, 600
    local single = '/env/tundra/props/trees/tund_pine01_s1_prop.bp'
    local group = CreatePropHPR('/env/tundra/props/trees/groups/tund_pine01_group1_prop.bp', x, GetTerrainHeight(x, z), z, 0, 0, 0)
    WaitTicks(1)
    local bones = group:GetBoneCount()
    -- No shipped blueprint sets SingleTreeBlueprint (mods do): give this
    -- group one.
    group.Blueprint = setmetatable({ SingleTreeBlueprint = single }, { __index = group.Blueprint })
    local ok, trees = pcall(group.Breakup, group)
    if not ok then WARN('SplitProp scenario: ' .. tostring(trees)) end
    check(ok, 'Breakup runs: ' .. tostring(trees))
    if ok then
        check(type(trees) == 'table' and table.getn(trees) == bones - 1, string.format('one tree per bone but the root: %s of %d', tostring(trees and table.getn(trees)), bones - 1))
        for _, t in trees or {} do
            check(t.Blueprint.BlueprintId == single, 'each is the single tree: ' .. tostring(t.Blueprint.BlueprintId))
        end
    end
    check(group:BeenDestroyed(), 'the group is gone')
    ScenarioResult.done = true
end)
