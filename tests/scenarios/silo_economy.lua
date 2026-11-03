-- Scenario test (tests/scenarios.rs): a missile's cost and rate come from
-- FAF's GetEnergyBuildAdjMod / GetMassBuildAdjMod (adjacency discounts) and
-- GetEconomyBuildRate, read when the missile starts.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local brain
local function drain(silo, ticks)
    local e0, m0 = brain:GetEconomyStored('ENERGY'), brain:GetEconomyStored('MASS')
    IssueSiloBuildTactical({ silo })
    WaitTicks(ticks)
    local progress = silo:GetWorkProgress()
    IssueClearCommands({ silo })
    silo:StopSiloBuild()
    WaitTicks(2)
    return e0 - brain:GetEconomyStored('ENERGY'), m0 - brain:GetEconomyStored('MASS'), progress
end
ForkThread(function()
    WaitTicks(2)
    brain = GetArmyBrain('ARMY_1')
    brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    local silo = CreateUnitHPR('ueb2108', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local e1, m1, p1 = drain(silo, 20)
    silo.EnergyBuildAdjMod, silo.MassBuildAdjMod = 0.5, 0.5
    local e2, m2, p2 = drain(silo, 20)
    note(string.format('full %.1f / %.1f, discounted %.1f / %.1f', e1, m1, e2, m2))
    check(e1 > 0 and math.abs(e2 - e1 / 2) < e1 * 0.05, 'half the energy with a 0.5 mod')
    check(m1 > 0 and math.abs(m2 - m1 / 2) < m1 * 0.05, 'half the mass with a 0.5 mod')
    silo.EnergyBuildAdjMod, silo.MassBuildAdjMod = nil, nil
    local rate = silo:GetBuildRate()
    silo.GetEconomyBuildRate = function(self) return rate * 2 end
    local _, _, p3 = drain(silo, 20)
    note(string.format('progress %.3f, at double rate %.3f', p1, p3))
    check(p3 > p1 * 1.8, 'double the rate, about double the progress')
    ScenarioResult.done = true
end)
