-- Scenario test (tests/scenarios.rs): entity:GetBoneDirection returns the
-- posed bone's forward axis (the Scathis turret, URL0401_Script.lua:104,
-- compares its barrels' directions with it).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local ok, x, y, z = pcall(tank.GetBoneDirection, tank, 0)
    check(ok, 'GetBoneDirection runs: ' .. tostring(x))
    if not ok then WARN('bone direction scenario: ' .. tostring(x)) ScenarioResult.done = true return end
    check(math.abs(x * x + y * y + z * z - 1) < 0.01, 'a unit vector')
    check(z > 0.99, string.format('an unturned tank faces +Z: %.2f %.2f %.2f', x, y, z))
    -- Turned a quarter: its forward axis turns with it.
    local turned = CreateUnitHPR('uel0201', 'ARMY_1', 410, GetTerrainHeight(410, 600), 600, 0, math.pi / 2, 0)
    WaitTicks(1)
    x, y, z = turned:GetBoneDirection(0)
    check(math.abs(x) > 0.99 and math.abs(z) < 0.05, string.format('turned: %.2f %.2f %.2f', x, y, z))
    -- The Scathis turret's aim script runs.
    local scathis = CreateUnitHPR('url0401', 'ARMY_1', 420, GetTerrainHeight(420, 620), 620, 0, 0, 0)
    WaitTicks(1)
    local okS, err = pcall(function() return scathis:GetBoneDirection('Turret_Aim') end)
    check(okS, 'the Scathis aim bone: ' .. tostring(err))
    ScenarioResult.done = true
end)
