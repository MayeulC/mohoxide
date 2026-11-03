-- Scenario test (tests/scenarios.rs): unit:CalculateWorldPositionFromRelative
-- turns an offset in the unit's frame into a world position (the Tempest
-- sends what it built behind itself, UAS0401_script.lua:195).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(1)
    local ok, p = pcall(tank.CalculateWorldPositionFromRelative, tank, { 0, 0, -10 })
    check(ok, 'it runs: ' .. tostring(p))
    if not ok then WARN('relative position scenario: ' .. tostring(p)) ScenarioResult.done = true return end
    check(math.abs(p[1] - 400) < 0.01 and math.abs(p[3] - 590) < 0.01, string.format('10 behind an unturned unit: %.2f, %.2f', p[1], p[3]))
    local turned = CreateUnitHPR('uel0201', 'ARMY_1', 420, GetTerrainHeight(420, 600), 600, 0, math.pi / 2, 0)
    WaitTicks(1)
    local q = turned:CalculateWorldPositionFromRelative({ 0, 0, -10 })
    local f = { turned:GetBoneDirection(0) }
    check(math.abs(q[1] - (420 - 10 * f[1])) < 0.05 and math.abs(q[3] - (600 - 10 * f[3])) < 0.05, string.format('behind a turned unit: %.2f, %.2f', q[1], q[3]))
    ScenarioResult.done = true
end)
