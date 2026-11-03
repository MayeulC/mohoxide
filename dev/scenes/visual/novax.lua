-- Novax defense satellite on station over an enemy base: the beam reaches
-- the ground from 75 up and burns the structures down.
-- cargo run -- --no-ai --no-audio --script dev/scenes/visual/novax.lua --camera 410,605,110
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("ENERGY", 500000) b:GiveResource("ENERGY", 500000)
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    at('xeb2402', 'ARMY_1', 400, 600)
    WaitTicks(150)
    at('ueb1301', 'ARMY_2', 420, 600)
    at('ueb1101', 'ARMY_2', 426, 612)
    at('ueb1101', 'ARMY_2', 414, 614)
end)
