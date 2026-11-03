-- Scenario test (tests/scenarios.rs): the Novax Center launches its defense
-- satellite, which flies at its 75 elevation and burns down a ground target.
-- Its beam reaches the ground because it's `MaximumBeamLength`
-- (200) long, not `MaxRadius` (32); the beam's end lands on the target's
-- collision box, which `DamageArea` (radius 0.5) hits as moho's does,
-- though the target's origin is further away. It holds its altitude while it
-- circles the target: the circling height change scales with
-- `AttackElevation` (0 for the satellite), not `Elevation`.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function ground(x, z) return GetTerrainHeight(x, z) end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("ENERGY", 500000) b:GiveResource("ENERGY", 500000)
    local novax = CreateUnitHPR('xeb2402', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local sat
    for _ = 1, 300 do
        WaitTicks(1)
        sat = novax.Satellite
        if sat and novax.waitingForLaunch == false then break end
    end
    check(sat ~= nil, 'the Novax Center made its satellite')
    if not sat then ScenarioResult.done = true return end
    -- Up to height: its controller (KLift 1, KLiftDamping 10) closes about a
    -- tenth of the gap a second, so ~50 s to be within a unit of 75.
    WaitTicks(500)
    -- A T3 power generator (9000 health) in range, for several beam bursts.
    local target = CreateUnitHPR('ueb1301', 'ARMY_2', 420, ground(420, 600), 600, 0, 0, 0)
    local lo, hi = math.huge, -math.huge
    local alo, ahi = math.huge, -math.huge
    for _ = 1, 600 do
        WaitTicks(1)
        if target.Dead or target:BeenDestroyed() then break end
        local p = sat:GetPosition()
        local y = p[2] - ground(p[1], p[3])
        lo, hi = math.min(lo, y), math.max(hi, y)
        alo, ahi = math.min(alo, p[2]), math.max(ahi, p[2])
    end
    note(string.format('satellite height above ground while engaging %.2f .. %.2f', lo, hi))
    check(target.Dead or target:BeenDestroyed(), 'the satellite burnt down the ground target')
    check(lo > 70, 'the satellite flies at its elevation: ' .. lo)
    -- Its slow controller holds its altitude as the ground varies below.
    -- The spread follows the seeded circling draws (0.5 .. 1.2 for targets
    -- made a few ticks either side of this one); a circling height change
    -- scaled by `Elevation` (75) swings it by tens.
    note(string.format('altitude %.2f .. %.2f', alo, ahi))
    check(ahi - alo < 2, string.format('the satellite holds its height while circling: %.2f .. %.2f', alo, ahi))
    note('target dead at tick ' .. GetGameTick())
    ScenarioResult.done = true
end)
