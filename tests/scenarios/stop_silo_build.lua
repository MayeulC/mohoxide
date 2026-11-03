-- Scenario test (tests/scenarios.rs): unit:StopSiloBuild drops the missile
-- in progress, as removing the UEF ACU's tactical missile enhancement does
-- (UEL0001_script.lua:666).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = GetArmyBrain('ARMY_1')
    brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    local silo = CreateUnitHPR('ueb2108', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local ends = 0
    local onEnd = silo.OnSiloBuildEnd
    silo.OnSiloBuildEnd = function(self, w) ends = ends + 1 return onEnd(self, w) end
    IssueSiloBuildTactical({ silo })
    WaitTicks(20)
    check(silo:IsUnitState('SiloBuildingAmmo'), 'building a missile')
    local ok, err = pcall(silo.StopSiloBuild, silo)
    check(ok, 'StopSiloBuild runs: ' .. tostring(err))
    if not ok then WARN('StopSiloBuild scenario: ' .. tostring(err)) end
    check(not silo:IsUnitState('SiloBuildingAmmo'), 'no longer building')
    check(ends == 1, 'OnSiloBuildEnd once: ' .. ends)
    WaitTicks(10)
    check(not silo:IsUnitState('SiloBuildingAmmo'), 'and it does not start again')
    check(silo:GetTacticalSiloAmmoCount() == 0, 'no missile')
    -- An adjacency buff mid-build restarts the missile with its progress
    -- (StructureUnit.OnReceiveAdjacencyBuffTo: StopSiloBuild, re-issue,
    -- GiveNukeSiloBlocks).
    IssueSiloBuildTactical({ silo })
    WaitTicks(30)
    local before = silo:GetWorkProgress()
    check(before > 0.05, string.format('missile under way: %.3f', before))
    silo:StopSiloBuild()
    IssueSiloBuildTactical({ silo })
    silo:GiveNukeSiloBlocks(before)
    -- 2 ticks (`WaitTicks(n)` waits n - 1) for the new build to take up the progress.
    WaitTicks(3)
    check(silo:GetTacticalSiloAmmoCount() == 0 and silo:GetNukeSiloAmmoCount() == 0, 'blocks are progress, not missiles')
    check(silo:GetWorkProgress() >= before - 0.01, string.format('progress kept: %.3f from %.3f', silo:GetWorkProgress(), before))
    -- The UEF ACU enhancement removal path.
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 420, GetTerrainHeight(420, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local bp = acu.Blueprint.Enhancements.TacticalMissile
    ok, err = pcall(acu.ProcessEnhancementTacticalMissileRemove, acu, bp)
    check(ok, 'removing the tactical missile enhancement runs: ' .. tostring(err))
    ScenarioResult.done = true
end)
