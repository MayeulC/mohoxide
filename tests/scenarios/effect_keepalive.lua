-- Scenario test (tests/scenarios.rs): the engine keeps live effects' script
-- objects alive (as moho does). FAF holds an engineer's build effects in a
-- weak-valued TrashBag; if a garbage collection could empty it, OnStopBuild
-- would never destroy the beams and they'd stay, stretched after the
-- engineer. Permanent decals (tarmacs) likewise.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local e = CreateUnitHPR('uel0105', 'ARMY_1', 420, GetTerrainHeight(420, 590), 590, 0, 0, 0)
    IssueBuildMobile({ e }, Vector(426, 0, 590), 'ueb1101', {})
    WaitTicks(40)
    local before = e.BuildEffectsBag and table.getn(e.BuildEffectsBag) or 0
    collectgarbage('collect')
    collectgarbage('collect')
    local after = e.BuildEffectsBag and table.getn(e.BuildEffectsBag) or 0
    note(string.format('build effects in the bag: %d before a full collection, %d after', before, after))
    check(before > 0, 'the engineer is building with effects')
    check(after == before, 'a garbage collection leaves the build effects in their bag')
    local pgen = e.UnitBeingBuilt
    WaitTicks(5)
    if pgen and pgen.TarmacBag and pgen.TarmacBag.Decals then
        local n = table.getn(pgen.TarmacBag.Decals)
        collectgarbage('collect')
        check(table.getn(pgen.TarmacBag.Decals) == n, 'and the tarmac decals in theirs')
    end
    ScenarioResult.done = true
end)
