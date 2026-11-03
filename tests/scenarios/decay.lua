-- Scenario test (tests/scenarios.rs): an unfinished structure nobody is
-- building decays (0.1 a tick of its highest cost, here a wall's 20 energy:
-- 0.5% a tick), and at nothing left gets OnDecayed (FAF destroys it). Work
-- on it holds the decay off.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    IssueBuildMobile({eng}, {403.5, GetTerrainHeight(403.5, 600.5), 600.5}, 'ueb5101', {})
    local wall
    for _ = 1, 100 do
        WaitTicks(1)
        wall = eng:GetFocusUnit()
        if wall and wall:GetFractionComplete() > 0.3 then break end
    end
    check(wall ~= nil, 'the engineer started the wall')
    if not wall then ScenarioResult.done = true return end
    IssueClearCommands({eng})
    WaitTicks(2)
    local f0, t0 = wall:GetFractionComplete(), GetGameTick()
    WaitTicks(10)
    local f1, ticks = wall:GetFractionComplete(), GetGameTick() - t0
    check(math.abs((f0 - f1) - 0.005 * ticks) < 0.002, string.format('decays 0.5%% a tick: %.4f -> %.4f in %d ticks', f0, f1, ticks))
    local decayed = false
    local old = wall.OnDecayed
    wall.OnDecayed = function(self) decayed = true return old(self) end
    for _ = 1, 200 do
        WaitTicks(1)
        if decayed then break end
    end
    check(decayed, 'OnDecayed at no progress left')
    WaitTicks(1)
    check(wall.Dead or IsDestroyed(wall), 'FAF destroys the decayed wall')
    ScenarioResult.done = true
end)
