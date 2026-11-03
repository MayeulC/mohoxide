-- Scenario test (tests/scenarios.rs): unit:ClearFocusEntity, which FAF's
-- DestroyShield calls (Unit.lua:4543) before destroying the shield.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local gen = CreateUnitHPR('ueb4202', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(5)
    check(gen.MyShield ~= nil, 'the generator has a shield')
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 410, GetTerrainHeight(410, 600), 600, 0, 0, 0)
    gen:SetFocusEntity(tank)
    check(gen:GetFocusUnit() == tank, 'focus set')
    local ok, err = pcall(gen.DestroyShield, gen)
    check(ok, 'DestroyShield runs: ' .. tostring(err))
    if not ok then WARN('clear focus scenario: ' .. tostring(err)) end
    check(gen:GetFocusUnit() == nil, 'focus cleared')
    check(gen.MyShield == nil, 'shield gone')
    ScenarioResult.done = true
end)
