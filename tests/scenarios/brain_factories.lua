-- Scenario test (tests/scenarios.rs): the campaign base managers' brain
-- queries: GetAvailableFactories([position, radius]) and
-- NumCurrentlyBuilding(buildeeCategory, builderCategory).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = GetArmyBrain('ARMY_1')
    local fac = CreateUnitHPR('ueb0101', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local function has(list, u) for _, v in list or {} do if v == u then return true end end return false end
    check(has(brain:GetAvailableFactories(), fac), 'an idle factory is available')
    check(has(brain:GetAvailableFactories({400, 0, 600}, 20), fac), 'within the radius')
    check(not has(brain:GetAvailableFactories({100, 0, 100}, 20), fac), 'not outside it')
    check(brain:NumCurrentlyBuilding(categories.uel0201, categories.FACTORY) == 0, 'building nothing yet')
    IssueBuildFactory({fac}, 'uel0201', 2)
    WaitTicks(10)
    check(not has(brain:GetAvailableFactories(), fac), 'a factory with orders is not available')
    check(brain:NumCurrentlyBuilding(categories.uel0201, categories.FACTORY) == 1, 'one factory building a tank: ' .. brain:NumCurrentlyBuilding(categories.uel0201, categories.FACTORY))
    check(brain:NumCurrentlyBuilding(categories.AIR, categories.FACTORY) == 0, 'not building air')
    ScenarioResult.done = true
end)
