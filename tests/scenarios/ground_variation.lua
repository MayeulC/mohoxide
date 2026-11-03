-- Scenario test (tests/scenarios.rs): a power generator (FlattenSkirt,
-- MaxGroundVariation 1) can't be placed on Seton's slope, only on flat
-- ground; a structure without FlattenSkirt checks the whole skirt.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    local flat = brain:CanBuildStructureAt('ueb1101', {380.5, 0, 580.5})
    local slope = brain:CanBuildStructureAt('ueb1101', {380.5, 0, 525.5})
    note(string.format('flat %s, slope %s (heights %.1f .. %.1f)', tostring(flat), tostring(slope), GetTerrainHeight(380, 520), GetTerrainHeight(380, 530)))
    check(flat, 'buildable on the plateau')
    check(not slope, 'not on the slope')
    done()
end)
