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
-- FAF's EnhanceTask (IssueScript) builds a commander enhancement through
-- its economy; it then applies (build rate 10 -> 42 for the T2 suite).
ForkThread(function()
    WaitTicks(2)
    rich(1)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    WaitTicks(5)
    check(acu:GetBuildRate() == 10, 'starts at build rate 10, got ' .. acu:GetBuildRate())
    IssueScript({acu}, { TaskName = 'EnhanceTask', Enhancement = 'AdvancedEngineering' })
    WaitTicks(20)
    check(acu:IsUnitState('Enhancing'), 'enhancing while the task runs')
    for _ = 1, 150 do
        WaitTicks(10)
        if acu:HasEnhancement('AdvancedEngineering') then break end
    end
    check(acu:HasEnhancement('AdvancedEngineering'), 'the enhancement completed')
    check(acu:GetBuildRate() == 42, 'build rate 42 after it, got ' .. acu:GetBuildRate())
    check(not acu:IsUnitState('Enhancing'), 'no longer enhancing')
    done()
end)
