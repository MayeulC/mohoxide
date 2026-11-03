-- Scenario test (tests/scenarios.rs): unit:GetNumBuildOrders(category),
-- which FAF's base managers read (BaseManager.lua): the units a factory's
-- queued build orders still make, in a category.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local fac = CreateUnitHPR('ueb0101', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    check(fac:GetNumBuildOrders(categories.ALLUNITS) == 0, 'nothing queued')
    IssueBuildFactory({fac}, 'uel0201', 3)
    IssueBuildFactory({fac}, 'uel0105', 2)
    WaitTicks(1)
    local all, tanks, air = fac:GetNumBuildOrders(categories.MOBILE), fac:GetNumBuildOrders(categories.uel0201), fac:GetNumBuildOrders(categories.AIR)
    check(all >= 4 and all <= 5, 'both orders counted (one may have started): ' .. all)
    check(tanks >= 2 and tanks <= 3, 'tanks only: ' .. tanks)
    check(air == 0, 'no air: ' .. air)
    ScenarioResult.done = true
end)
