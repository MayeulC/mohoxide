-- Galactic Colossus eye beam and tractor claws on a few tanks (beam
-- direction, start / end effects).
-- cargo run -- --no-ai --script dev/scenes/visual/colossus_beam.lua --camera 420,600,45
ForkThread(function()
    WaitTicks(2)
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    at('ual0401', 'ARMY_1', 410, 590)
    for i = 1, 4 do at('uel0201', 'ARMY_2', 420 + i * 4, 612) end
end)
