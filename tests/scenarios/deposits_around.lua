-- Scenario test (tests/scenarios.rs): GetDepositsAroundPoint finds the
-- deposits near a point, by type, with their square and distance.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(1)
    CreateResourceDeposit('Mass', 700.5, GetTerrainHeight(700, 700), 700.5, 1)
    CreateResourceDeposit('Hydrocarbon', 710, GetTerrainHeight(710, 700), 700, 3)
    local all = GetDepositsAroundPoint(700, 700, 15, 0)
    check(table.getn(all) == 2, 'both within 15: ' .. table.getn(all))
    local mass = GetDepositsAroundPoint(700, 700, 15, 1)
    check(table.getn(mass) == 1 and mass[1].Type == 1, 'one mass')
    check(mass[1] and math.abs(mass[1].X1 - 700) < 0.01 and math.abs(mass[1].X2 - 701) < 0.01, 'its square: ' .. tostring(mass[1] and mass[1].X1))
    local hydro = GetDepositsAroundPoint(700, 700, 15, 2)
    check(table.getn(hydro) == 1 and math.abs(hydro[1].Dist - 10) < 0.01, 'hydro 10 away')
    check(table.getn(GetDepositsAroundPoint(700, 700, 5, 2)) == 0, 'not within 5')
    ScenarioResult.done = true
end)
