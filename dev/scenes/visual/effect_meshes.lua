-- The effect-mesh techniques in action: one strategic missile per faction at
-- open ground (nuke heads, explosion, clouds, EMP / quantum effects) and
-- artillery shelling UEF and Cybran shields (shield impacts).
-- cargo run -- --no-ai --script dev/scenes/visual/effect_meshes.lua --camera 300,620,120
ForkThread(function()
    WaitTicks(2)
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    for _, b in ArmyBrains do
        b:__engine_SetStorage('MASS', 100000) b:__engine_SetStorage('ENERGY', 1000000)
        b:GiveResource('MASS', 100000) b:GiveResource('ENERGY', 1000000)
    end

    -- Shields under fire.
    local uef = at('ueb4202', 'ARMY_1', 400, 600)
    local cyb = at('urb4202', 'ARMY_1', 430, 600)
    for i = 1, 3 do
        at('uel0103', 'ARMY_2', 395 + i * 5, 625)
        at('url0103', 'ARMY_2', 425 + i * 5, 625)
    end

    -- Strategic launchers, then targets spread over open ground.
    local launchers = { 'ueb2305', 'uab2305', 'urb2305', 'xsb2305' }
    local targets = { { 260, 560 }, { 300, 620 }, { 340, 560 }, { 300, 680 } }
    for i, bp in launchers do
        local l = at(bp, 'ARMY_1', 360 + i * 12, 520)
        l:GiveNukeSiloAmmo(1)
        WaitTicks(5)
        IssueNuke({ l }, Vector(targets[i][1], 0, targets[i][2]))
    end
end)
