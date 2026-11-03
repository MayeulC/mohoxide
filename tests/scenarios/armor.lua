-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    return b
end
-- Damage goes through armordefinition.lua: structures take 3.2% of Deathnuke.
ForkThread(function()
    WaitTicks(2)
    local pgen = CreateUnitHPR('ueb1101', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 420, ground(420, 600), 600, 0, 0, 0)
    check(pgen:GetArmorMult('Overcharge') == 0.25, 'structure Overcharge 0.25')
    check(pgen:GetArmorMult('Normal') == 1, 'structure Normal 1')
    check(tank:GetArmorMult('Normal') == 1, 'tank Normal 1')
    local h0 = pgen:GetHealth()
    Damage(nil, pgen:GetPosition(), pgen, 100, 'Deathnuke')
    WaitTicks(1)
    local lost = h0 - pgen:GetHealth()
    check(math.abs(lost - 3.2) < 0.01, '100 Deathnuke took 3.2, took ' .. lost)
    done()
end)
