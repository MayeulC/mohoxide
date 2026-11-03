-- Scenario test (tests/scenarios.rs): a builder's script vetoes builds with
-- CheckBuildRestriction(bp), asked before a build starts.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local b = GetArmyBrain('ARMY_1')
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local factory = CreateUnitHPR('ueb0101', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local asked = {}
    local orig = factory.CheckBuildRestriction
    factory.CheckBuildRestriction = function(self, bp)
        table.insert(asked, bp.BlueprintId)
        if bp.BlueprintId == 'uel0105' then return false end
        return orig(self, bp)
    end
    local failed = 0
    local onFailed = factory.OnFailedToBuild
    factory.OnFailedToBuild = function(self) failed = failed + 1 return onFailed(self) end
    IssueBuildFactory({ factory }, 'uel0105', 1)
    WaitTicks(5)
    check(asked[1] == 'uel0105', 'asked about the engineer: ' .. table.concat(asked, ' '))
    check(factory:GetFocusUnit() == nil, 'refused: nothing being built')
    check(failed == 1, 'OnFailedToBuild once: ' .. failed)
    IssueBuildFactory({ factory }, 'uel0201', 1)
    WaitTicks(5)
    check(factory:GetFocusUnit() ~= nil, 'a tank is allowed and started')
    ScenarioResult.done = true
end)
