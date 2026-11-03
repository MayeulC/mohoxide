-- Scenario test (tests/scenarios.rs): the sim's toggle methods, as moho's
-- bindings do them: GetScriptBit / ToggleScriptBit (only with the toggle
-- cap), ToggleFireState's cycle, GetShieldRatio.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local gen = CreateUnitHPR('ueb4202', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(5)
    local before = gen:GetScriptBit('RULEUTC_ShieldToggle')
    gen:ToggleScriptBit('RULEUTC_ShieldToggle')
    check(gen:GetScriptBit('RULEUTC_ShieldToggle') == not before, 'shield toggle flips')
    gen:ToggleScriptBit('RULEUTC_ShieldToggle')
    check(gen:GetScriptBit('RULEUTC_ShieldToggle') == before, 'and flips back')
    gen:ToggleScriptBit('RULEUTC_CloakToggle')
    check(not gen:GetScriptBit('RULEUTC_CloakToggle'), 'no toggle cap: unchanged')

    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 410, GetTerrainHeight(410, 600), 600, 0, 0, 0)
    local states = {}
    for i = 1, 4 do tank:ToggleFireState() states[i] = tank:GetFireState() end
    check(states[1] == 1 and states[2] == 2 and states[3] == 0 and states[4] == 1, 'fire state cycles 1,2,0,1: got ' .. table.concat(states, ','))

    gen:SetShieldRatio(0.5)
    check(math.abs(gen:GetShieldRatio() - 0.5) < 1e-3, 'shield ratio reads back: ' .. tostring(gen:GetShieldRatio()))
    ScenarioResult.done = true
end)
