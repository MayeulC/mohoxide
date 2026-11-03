-- Picture-in-picture popups: army 1 starts an experimental by its ACU
-- (tick ~20), then its ACU dies (tick 70); each asks for a popup.
-- cargo run -- --session --no-ai --no-audio --script dev/scenes/ui/pip.lua --screenshot pip.png --screenshot-tick 90
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 500000) b:__engine_SetStorage('ENERGY', 5000000)
    b:GiveResource('MASS', 500000) b:GiveResource('ENERGY', 5000000)
    local x, z = b:GetArmyStartPos()
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
    local p = acu:GetPosition()
    local e = CreateUnitHPR('uel0309', 'ARMY_1', p[1] + 6, GetTerrainHeight(p[1] + 6, p[3]), p[3], 0, 0, 0)
    IssueBuildMobile({ e }, { p[1] + 20, GetTerrainHeight(p[1] + 20, p[3]), p[3] }, 'uel0401', {})
    WaitTicks(68)
    LOG('dev/pip: killing the ACU at tick ', GetGameTick())
    acu:Kill()
end)
