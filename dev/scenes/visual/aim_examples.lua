-- Turret aim examples, each pair its own two armies (the rest
-- allied), targets on hold fire and too tough to die:
-- 1. walkers whose walk clip sways the bone their turret sits on, walking
--    back and forth past a tank 10 to their side: the Mantis (url0107, its
--    torso) at z 790, the UEF ACU (uel0001) at z 850;
-- 2. a T1 tank on Seton's slope at (384, 696), its hull rolled, aiming at a
--    tank 15 east: its aim controller's GetHeadingPitch every second,
--    printed on screen and logged (AIMEXAMPLE lines);
-- 3. a UEF ACU at (360, 740) aiming its gun at a tank 15 east, then ordered
--    at tick 80 to build a power generator 12 west: FAF hands the right arm
--    from the gun to the build arm with SetHeadingPitch(GetHeadingPitch()).
-- cargo run -- --no-ai --armies 8 --fog none --script dev/scenes/visual/aim_examples.lua
-- with a low camera through --ui-lua (it runs once the interface has been
-- up 6 s, so screenshot at tick 70 or later), e.g. for the slope tank
--   --ui-lua "GetCamera('WorldCamera'):SnapTo({386, 21, 696}, {math.pi, 0.3, 0}, 14)"
ForkThread(function()
    WaitTicks(2)
    for a = 1, 8 do
        for b = 1, 8 do
            if a ~= b then SetAlliance(a, b, 'Ally') end
        end
        local brain = GetArmyBrain('ARMY_' .. a)
        brain:__engine_SetStorage('ENERGY', 500000) brain:GiveResource('ENERGY', 500000)
        brain:__engine_SetStorage('MASS', 50000) brain:GiveResource('MASS', 50000)
    end
    SetAlliance(1, 2, 'Enemy') SetAlliance(3, 4, 'Enemy') SetAlliance(5, 6, 'Enemy') SetAlliance(7, 8, 'Enemy')
    local function at(bp, army, x, z) return CreateUnitHPR(bp, 'ARMY_' .. army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local function target(bp, army, x, z)
        local t = at(bp, army, x, z)
        t:SetMaxHealth(1e7) t:SetHealth(nil, 1e7) t:SetFireState(1)
        return t
    end
    -- 1. Walkers.
    for _, w in { { 'url0107', 1, 790 }, { 'uel0001', 3, 850 } } do
        local u = at(w[1], w[2], 300, w[3])
        u:SetMaxHealth(1e7) u:SetHealth(nil, 1e7)
        target('uel0201', w[2] + 1, 320, w[3] + 10)
        for k = 1, 8 do
            local x = (math.mod(k, 2) == 1) and 340 or 300
            IssueMove({ u }, { x, GetTerrainHeight(x, w[3]), w[3] })
        end
    end
    -- 2. The tank on the slope.
    local tank = at('uel0201', 5, 384, 696)
    target('uel0201', 6, 399, 696)
    -- 3. The ACU handing its arm over.
    local acu = at('uel0001', 7, 360, 740)
    target('uel0201', 8, 375, 740)
    ForkThread(function()
        WaitTicks(78)
        IssueBuildMobile({ acu }, { 348, GetTerrainHeight(348, 740), 740 }, 'ueb1101', {})
    end)
    for _ = 1, 30 do
        WaitTicks(10)
        local aim = tank:GetWeapon(1).AimControl
        if aim then
            local h, p = aim:GetHeadingPitch()
            local x, y, z = tank:GetBoneDirection('Turret_Muzzle')
            local text = string.format('tank GetHeadingPitch %.1f / %.1f, muzzle %.1f up', math.deg(h), math.deg(p), math.deg(math.asin(y)))
            LOG('AIMEXAMPLE t=', GetGameTick(), ' ', text)
            if PrintText then PrintText(text, 24, 'ffffffff', 1, 'centertop') end
        end
        local gun = acu:GetWeaponManipulatorByLabel(acu.rightGunLabel or 'RightZephyr')
        if gun then
            local h, p = gun:GetHeadingPitch()
            LOG('AIMEXAMPLE t=', GetGameTick(), string.format(' acu gun GetHeadingPitch %.1f / %.1f', math.deg(h), math.deg(p)))
        end
    end
end)
