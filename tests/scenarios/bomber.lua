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
-- A T1 bomber flies attack runs at a tank and kills it in one pass.
ForkThread(function()
    WaitTicks(2)
    local b = CreateUnitHPR('uea0103', 'ARMY_1', 380, 40, 600, 0, 0, 0)
    local t = CreateUnitHPR('uel0201', 'ARMY_2', 440, ground(440, 600), 600, 0, 0, 0)
    IssueStop({t})
    IssueAttack({b}, t)
    local passes, close = 0, false
    for _ = 1, 400 do
        WaitTicks(1)
        if t.Dead or t:BeenDestroyed() then break end
        local p, q = b:GetPosition(), t:GetPosition()
        local d = VDist2(p[1], p[3], q[1], q[3])
        if d < 5 and not close then passes = passes + 1 close = true elseif d > 25 then close = false end
    end
    check(t.Dead or t:BeenDestroyed(), 'the tank was bombed to death')
    check(passes >= 1 and passes <= 2, 'within one or two passes, took ' .. passes)
    check(b:GetCurrentLayer() == 'Air', 'the bomber stayed in the air')
    done()
end)
