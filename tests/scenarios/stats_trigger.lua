-- Scenario test (tests/scenarios.rs): brain:RemoveArmyStatsTrigger removes a
-- trigger by name, as the AI energy manager does
-- (EnergyManagerBrainComponent.lua:278): a removed trigger never fires.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = GetArmyBrain('ARMY_1')
    local fired = {}
    brain.OnStatsTrigger = function(self, name) table.insert(fired, name) end
    brain:SetArmyStatsTrigger('Economy_Stored_Energy', 'Kept', 'GreaterThanOrEqual', 0)
    brain:SetArmyStatsTrigger('Economy_Ratio_Energy', 'Removed', 'GreaterThanOrEqual', 0)
    -- Removed under another stat name: the name is what counts.
    local ok, err = pcall(brain.RemoveArmyStatsTrigger, brain, 'Economy_Stored_Energy', 'Removed')
    check(ok, 'RemoveArmyStatsTrigger runs: ' .. tostring(err))
    if not ok then WARN('stats trigger scenario: ' .. tostring(err)) end
    WaitTicks(5)
    check(table.concat(fired, ' ') == 'Kept', 'only the kept trigger fired: ' .. table.concat(fired, ' '))
    ScenarioResult.done = true
end)
