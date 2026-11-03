-- Scenario test (tests/scenarios.rs): unit:SetPaused fires OnPaused /
-- OnUnpaused when the flag changes, and FAF's handlers stop and restart a
-- builder's consumption.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = GetArmyBrain('ARMY_1')
    brain:GiveResource('MASS', 10000)
    brain:GiveResource('ENERGY', 100000)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local calls = {}
    local onPaused, onUnpaused = eng.OnPaused, eng.OnUnpaused
    eng.OnPaused = function(self) table.insert(calls, 'paused') return onPaused(self) end
    eng.OnUnpaused = function(self) table.insert(calls, 'unpaused') return onUnpaused(self) end
    IssueBuildMobile({ eng }, { 410, GetTerrainHeight(410, 600), 600 }, 'ueb1101', {})
    local t = 0
    while not eng:IsUnitState('Building') and t < 200 do WaitTicks(1) t = t + 1 end
    check(eng:IsUnitState('Building'), 'the engineer is building')
    WaitTicks(5)
    local function consuming() return eng:GetResourceConsumed() > 0 and eng.ActiveConsumption end
    check(eng.ActiveConsumption, 'consumption is on while building')
    eng:SetPaused(true)
    check(calls[1] == 'paused', 'SetPaused(true) calls OnPaused: ' .. table.concat(calls, ' '))
    check(not eng.ActiveConsumption, 'FAF turned consumption off')
    eng:SetPaused(true)
    check(table.getn(calls) == 1, 'no call when the flag does not change: ' .. table.concat(calls, ' '))
    eng:SetPaused(false)
    check(calls[2] == 'unpaused', 'SetPaused(false) calls OnUnpaused: ' .. table.concat(calls, ' '))
    check(eng.ActiveConsumption, 'FAF turned consumption back on')
    ScenarioResult.done = true
end)
