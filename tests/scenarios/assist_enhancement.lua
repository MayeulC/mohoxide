-- Scenario test (tests/scenarios.rs): engineers assisting a commander's
-- enhancement speed it up. Moho's repair task on an enhancing unit calls
-- InheritWork(target) once, adds the assister's share to the target's
-- WorkProgress each tick, and ends with ClearWork().
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
end
local function enhance(x, helpers)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', x, GetTerrainHeight(x, 600), 600, 0, 0, 0)
    local log = {}
    local engs = {}
    for i = 1, helpers do
        local e = CreateUnitHPR('uel0105', 'ARMY_1', x + 3, GetTerrainHeight(x + 3, 600 + i * 2), 600 + i * 2, 0, 0, 0)
        local inherit, clear = e.InheritWork, e.ClearWork
        e.InheritWork = function(self, t) table.insert(log, 'inherit') return inherit(self, t) end
        e.ClearWork = function(self) table.insert(log, 'clear') return clear(self) end
        table.insert(engs, e)
    end
    WaitTicks(5)
    IssueScript({ acu }, { TaskName = 'EnhanceTask', Enhancement = 'AdvancedEngineering' })
    WaitTicks(3)
    if helpers > 0 then IssueRepair(engs, acu) end
    local ticks = 0
    while not acu:HasEnhancement('AdvancedEngineering') and ticks < 3000 do WaitTicks(1) ticks = ticks + 1 end
    WaitTicks(3)
    return ticks, log, acu, engs
end
ForkThread(function()
    WaitTicks(2)
    rich(1)
    local alone = enhance(380, 0)
    local helped, log, acu, engs = enhance(420, 2)
    note(string.format('alone %d ticks, with two engineers %d', alone, helped))
    check(acu:HasEnhancement('AdvancedEngineering'), 'the helped enhancement completed')
    check(helped < alone * 0.8, string.format('faster with help: %d vs %d', helped, alone))
    local s = table.concat(log, ' ')
    local inherits = select(2, string.gsub(s, 'inherit', ''))
    local clears = select(2, string.gsub(s, 'clear', ''))
    check(inherits >= 2, 'each engineer inherited the work: ' .. s)
    check(clears >= 2, 'each task ended with ClearWork: ' .. s)
    check(table.getn(engs[1]:GetCommandQueue()) == 0, 'the repair order is done')
    ScenarioResult.done = true
end)
