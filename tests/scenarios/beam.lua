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
-- A beam weapon (Seraphim Lightning Tank's phason beam) fires its collision
-- beam at a tank in range: the beam is created, enables, hits the tank
-- (OnImpact 'Unit') and kills it; beams never hit the launcher's own side.
ForkThread(function()
    WaitTicks(2)
    local laser = CreateUnitHPR('dslk004', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local friend = CreateUnitHPR('uel0201', 'ARMY_1', 407, ground(407, 600), 600, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_2', 415, ground(415, 600), 600, 0, 0, 0)
    IssueStop({laser, friend, tank})
    -- Only the beam may do damage: the tanks hold fire (they would
    -- otherwise shell each other at point blank).
    friend:SetFireState(1)
    tank:SetFireState(1)
    local w = laser:GetWeaponByLabel('PhasonBeamGround')
    check(w and w.Beams and table.getn(w.Beams) >= 1, 'the beam weapon created its collision beam(s)')
    check(w and not w.Beams[1].Beam:IsEnabled(), 'beams start disabled')
    local hp0 = friend:GetHealth()
    for _ = 1, 200 do
        WaitTicks(1)
        if tank.Dead or tank:BeenDestroyed() then break end
    end
    check(tank.Dead or tank:BeenDestroyed(), 'the enemy tank was burnt down by the beam')
    check(friend:GetHealth() == hp0, 'the friendly tank in the line of fire was not hurt')
    note('beam kill at tick ' .. GetGameTick())
    done()
end)
