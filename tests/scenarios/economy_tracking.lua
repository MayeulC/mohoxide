-- Scenario test (tests/scenarios.rs): OnProductionActive / InActive and
-- OnConsumptionActive / InActive when the setters change the state, and a
-- weapon's OnStartTracking(label) / OnStopTracking(label) with its target.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local calls = {}
    local pgen = CreateUnitHPR('ueb1101', 'ARMY_1', 380, GetTerrainHeight(380, 560), 560, 0, 0, 0)
    for _, name in { 'OnProductionActive', 'OnProductionInActive', 'OnConsumptionActive', 'OnConsumptionInActive' } do
        local orig = pgen[name]
        pgen[name] = function(self) table.insert(calls, name); return orig(self) end
    end
    WaitTicks(2)
    calls = {}
    pgen:SetProductionActive(false)
    pgen:SetProductionActive(false)
    pgen:SetProductionActive(true)
    pgen:SetConsumptionActive(true)
    pgen:SetConsumptionActive(false)
    local got = table.concat(calls, ' ')
    check(got == 'OnProductionInActive OnProductionActive OnConsumptionActive OnConsumptionInActive',
        'one call per change: ' .. got)

    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local w = tank:GetWeapon(1)
    local tracking = {}
    local start, stop = w.OnStartTracking, w.OnStopTracking
    w.OnStartTracking = function(self, label) table.insert(tracking, 'start:' .. tostring(label)); return start(self, label) end
    w.OnStopTracking = function(self, label) table.insert(tracking, 'stop:' .. tostring(label)); return stop(self, label) end
    local enemy = CreateUnitHPR('uel0201', 'ARMY_2', 410, GetTerrainHeight(410, 600), 600, 0, 0, 0)
    for _ = 1, 50 do
        WaitTicks(1)
        if tracking[1] then break end
    end
    check(tracking[1] and string.find(tracking[1], '^start:') and tracking[1] ~= 'start:nil',
        'started tracking with a label: ' .. table.concat(tracking, ' '))
    enemy:Destroy()
    for _ = 1, 50 do
        WaitTicks(1)
        if tracking[2] then break end
    end
    check(tracking[2] and string.find(tracking[2], '^stop:'), 'stopped tracking when the target went: ' .. table.concat(tracking, ' '))
    ScenarioResult.done = true
end)
