-- Scenario test (tests/scenarios.rs): a turret with nothing left to shoot
-- goes back to rest. An ACU turns its torso to a tank off to its side; once
-- the tank is gone the torso swings back to face the way the ACU does
-- (moho: after the weapon's TargetCheckInterval when no SetResetPoseTime
-- was given; structures, which FAF gives 9999999 s, hold their aim).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
-- Degrees the torso is turned from the ACU's facing (it faces +z).
local function torso_turn(u)
    local x, _, z = u:GetBoneDirection('Torso')
    return math.deg(math.atan2(x, z))
end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
        WaitTicks(5)
        local rest = torso_turn(acu)
        local tank = CreateUnitHPR('uel0201', 'ARMY_2', 415, GetTerrainHeight(415, 600), 600, 0, 0, 0)
        WaitTicks(30)
        local aimed = torso_turn(acu)
        tank:Destroy()
        WaitTicks(60)
        local after = torso_turn(acu)
        note(string.format('torso %.1f at rest, %.1f aiming at the tank, %.1f after it was gone', rest, aimed, after))
        check(math.abs(aimed - rest) > 30, 'the torso turned to the tank')
        check(math.abs(after - rest) < 2, 'and went back to rest once it was gone')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
