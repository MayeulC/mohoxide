-- Scenario test (tests/scenarios.rs): extractors and deposits (moho's
-- OCCUPY_Check, MohoEngine.dll 0x10153ab0). An extractor's footprint cells,
-- from round(pos - size / 2) with halves to even, and its deposit's (the
-- same from the marker) contain one another: a mass extractor stands on its
-- deposit's cell, never half on it. Other structures keep their skirts off
-- deposits grown by half a cell (mass) or 1.5 (hydrocarbon).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    local function can(bp, x, z) return brain:CanBuildStructureAt(bp, {x, GetTerrainHeight(x, z), z}) end
    -- A deposit on a cell centre: only that cell.
    CreateResourceDeposit('Mass', 400.5, GetTerrainHeight(400.5, 600.5), 600.5, 1)
    check(can('ueb1103', 400.5, 600.5), 'an extractor on the deposit')
    check(not can('ueb1103', 401.5, 600.5), 'not a cell over')
    check(not can('ueb1103', 400.5, 599.5), 'nor a cell over the other way')
    -- A deposit on a cell corner covers the even cell of the two.
    CreateResourceDeposit('Mass', 410, GetTerrainHeight(410, 610), 610, 1)
    check(can('ueb1103', 410.5, 610.5), 'the even cell of a deposit on a whole number')
    check(not can('ueb1103', 409.5, 609.5), 'not the odd one')
    -- Storages ring an extractor but don't cover its deposit.
    check(can('ueb1106', 402.5, 600.5), 'a storage two cells out')
    check(not can('ueb1106', 401.5, 600.5), 'a storage on the deposit cell next door is refused')
    check(not can('ueb1101', 400.5, 600.5), 'a power generator on a deposit is refused')
    ScenarioResult.done = true
end)
