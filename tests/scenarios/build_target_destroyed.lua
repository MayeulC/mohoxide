-- Scenario test (tests/scenarios.rs): a structure destroyed while it is
-- being built stops its builder and its assister with OnStopBuild(target),
-- after the builder's OnFailedToBuild (moho's build stop), so FAF's
-- StopBuildingEffects takes their build beams down. Killed from a
-- script thread, after the construction pass: the stop comes at the end of
-- that tick, while the destroyed structure can still be passed.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local b = GetArmyBrain('ARMY_1')
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local function engineer(x)
        local e = CreateUnitHPR('uel0105', 'ARMY_1', x, GetTerrainHeight(x, 600), 600, 0, 0, 0)
        e.log = {}
        local stopBuild, failed = e.OnStopBuild, e.OnFailedToBuild
        e.OnStopBuild = function(self, built, order)
            table.insert(self.log, 'stop')
            self.stoppedOn = built
            return stopBuild(self, built, order)
        end
        e.OnFailedToBuild = function(self)
            table.insert(self.log, 'failed')
            return failed(self)
        end
        return e
    end
    local eng, helper = engineer(400), engineer(396)
    -- A power generator (not a mass extractor: no deposit needed).
    local x, z = 410.5, 600.5
    IssueBuildMobile({ eng }, { x, GetTerrainHeight(x, z), z }, 'ueb1101', {})
    local built
    for _ = 1, 200 do
        WaitTicks(1)
        built = eng:GetFocusUnit()
        if built then break end
    end
    check(built ~= nil, 'the build started')
    if not built then ScenarioResult.done = true return end
    IssueGuard({ helper }, eng)
    for _ = 1, 100 do
        WaitTicks(1)
        if helper:GetFocusUnit() == built then break end
    end
    check(helper:GetFocusUnit() == built, 'the assister works on it')
    WaitTicks(5)
    check(built:GetFractionComplete() < 1, 'still unfinished: ' .. built:GetFractionComplete())
    check(not eng.BuildEffectsBag:Empty(), 'build beams up while building')
    built:Kill()
    -- (FAF's death thread destroys it a little later.)
    for _ = 1, 100 do
        WaitTicks(1)
        if IsDestroyed(built) then break end
    end
    check(IsDestroyed(built), 'the structure is destroyed')
    WaitTicks(2)
    check(table.concat(eng.log, ' ') == 'failed stop', 'builder: OnFailedToBuild then OnStopBuild once: ' .. table.concat(eng.log, ' '))
    check(eng.stoppedOn == built, 'builder stopped on the destroyed structure')
    check(eng.BuildEffectsBag:Empty(), 'builder beams gone')
    check(eng:GetFocusUnit() == nil, 'builder lost its focus')
    check(table.concat(helper.log, ' ') == 'stop', 'assister: OnStopBuild once: ' .. table.concat(helper.log, ' '))
    check(helper.stoppedOn == built, 'assister stopped on the destroyed structure')
    check(helper.BuildEffectsBag:Empty(), 'assister beams gone')
    ScenarioResult.done = true
end)
