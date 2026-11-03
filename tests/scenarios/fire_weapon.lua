-- Scenario test (tests/scenarios.rs): weapon:FireWeapon fires at once. The
-- fire beetle's detonate toggle (XRL0302_Script.lua:188, script bit 4)
-- fires its Suicide weapon: it blows up and hurts what's next to it.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local beetle = CreateUnitHPR('xrl0302', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_2', 402, GetTerrainHeight(402, 600), 600, 0, 0, 0)
    WaitTicks(3)
    local before = tank:GetHealth()
    local ok, err = pcall(beetle.SetScriptBit, beetle, 'RULEUTC_ProductionToggle', true)
    check(ok, 'detonating runs: ' .. tostring(err))
    if not ok then WARN('fire weapon scenario: ' .. tostring(err)) end
    WaitTicks(2)
    check(beetle:BeenDestroyed(), 'the beetle is gone')
    check(tank.Dead or tank:GetHealth() < before, string.format('the tank was hurt: %s -> %s', before, tank.Dead and 'dead' or tank:GetHealth()))
    ScenarioResult.done = true
end)
