-- Scenario test (tests/scenarios.rs): unit:RestoreBuildRestrictions clears
-- the unit's own restrictions; removing the Aeon ACU's engineering
-- enhancement (UAL0001_script.lua:219) restores, then re-restricts.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local acu = CreateUnitHPR('ual0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local t2 = 'uab1201'
    check(not acu:CanBuild(t2), 'a new ACU cannot build tech 2')
    acu:RestoreBuildRestrictions()
    check(acu:CanBuild(t2), 'restored: tech 2 is buildable again')
    local bp = acu.Blueprint.Enhancements.AdvancedEngineering
    local ok, err = pcall(acu.ProcessEnhancementAdvancedEngineeringRemove, acu, bp)
    check(ok, 'removing the engineering enhancement runs: ' .. tostring(err))
    check(not acu:CanBuild(t2), 'after removal tech 2 is restricted again')
    if not ok then WARN('RestoreBuildRestrictions scenario: ' .. tostring(err)) end
    ScenarioResult.done = true
end)
