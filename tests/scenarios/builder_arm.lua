-- Scenario test (tests/scenarios.rs): builder arms turn toward what their
-- unit builds and back to rest when it's done. A UEF ACU facing +z builds a
-- wall segment off its side; its BuildArmManipulator (FAF's CommandUnit)
-- heads toward it while building.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local arm = acu.BuildArmManipulator
    check(arm ~= nil, 'the ACU has a builder arm')
    if not arm then ScenarioResult.done = true return end
    local h0 = arm:GetHeadingPitch()
    -- The torso turns (FAF's OnPrepareArmToBuild gives the builder arm the
    -- torso over the guns): the left arm's muzzle swings toward the site.
    local m0 = acu:GetPosition('Left_Arm_Muzzle')
    -- A power generator 6 to the ACU's side (-x).
    IssueBuildMobile({acu}, {394.5, GetTerrainHeight(394.5, 600.5), 600.5}, 'ueb1101', {})
    local peak, swing = 0, 0
    for _ = 1, 60 do
        WaitTicks(1)
        local h = arm:GetHeadingPitch()
        if math.abs(h) > math.abs(peak) then peak = h end
        local m = acu:GetPosition('Left_Arm_Muzzle')
        swing = math.min(swing, m[1] - m0[1])
    end
    note(string.format('muzzle swing toward the site %.2f', swing))
    check(swing < -0.5, 'the torso turns toward the build site (the arm muzzle swings to -x)')
    note(string.format('rest heading %.2f, peak while building %.2f', h0, peak))
    check(math.abs(h0) < 0.05, 'the arm starts at rest')
    check(math.abs(peak) > 0.5, 'the arm turns toward the build site while building')
    -- Wait for it to finish, then the arm returns to rest.
    for _ = 1, 600 do
        WaitTicks(1)
        if not acu:IsUnitState('Building') then break end
    end
    WaitTicks(40)
    local h = arm:GetHeadingPitch()
    check(math.abs(h) < 0.05, string.format('the arm is back at rest after building (%.2f)', h))
    ScenarioResult.done = true
end)
