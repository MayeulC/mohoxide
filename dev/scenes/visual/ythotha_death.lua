-- Ythotha (XSL0401) death: its wreck, and the energy being (XSL0402) it
-- spawns at about tick 74 (a crackling ball: emitters on its hidden bones)
-- zapping four tanks placed once the Ythotha is dying (so it can't shoot
-- them; they can't be killed, so the being keeps firing).
-- cargo run -- --no-ai --script dev/scenes/visual/ythotha_death.lua --camera 414,594,55
ForkThread(function()
    WaitTicks(2)
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local ythotha = at('xsl0401', 'ARMY_1', 410, 590)
    WaitTicks(7)
    LOG('dev/ythotha_death: killing the Ythotha at tick ', GetGameTick())
    ythotha:Kill()
    WaitTicks(2)
    for i = 1, 4 do at('uel0201', 'ARMY_2', 416 + i * 2, 596):SetCanTakeDamage(false) end
    -- The being only targets what its army sees.
    at('xsl0101', 'ARMY_1', 385, 590)
end)
