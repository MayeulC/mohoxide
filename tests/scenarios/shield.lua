-- Scenario test (tests/scenarios.rs): shield bubbles stop enemy shots.
-- A UEF shield generator (bubble radius 13) covers a tank near its edge; an
-- enemy tank outside the bubble shoots at it: the shots hit the bubble
-- (OnCollisionCheck says yes for enemies, impact 'Shield'), the shield loses
-- health, the covered tank none.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function make(bp, army, x, z, h) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, h or 0, 0) end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    brain:__engine_SetStorage('ENERGY', 500000) brain:GiveResource('ENERGY', 500000)
    local gen = make('ueb4202', 'ARMY_1', 400.5, 600.5)
    local wall = make('uel0201', 'ARMY_1', 400.5, 610.5)
    WaitTicks(60)
    local shield = gen.MyShield
    check(shield ~= nil, 'the generator made a shield')
    if not shield then ScenarioResult.done = true return end
    local s0 = shield:GetHealth()
    local tank = make('uel0201', 'ARMY_2', 400.5, 625.5, 3.14159)
    IssueAttack({tank}, wall)
    WaitTicks(120)
    local s1, w1 = shield:GetHealth(), wall:GetHealth() / wall:GetMaxHealth()
    note(string.format('shield health %.0f -> %.0f, covered tank %.2f', s0, s1, w1))
    check(s1 < s0, 'the shield took the shots')
    check(w1 >= 0.999, 'the tank under it took none')
    ScenarioResult.done = true
end)
