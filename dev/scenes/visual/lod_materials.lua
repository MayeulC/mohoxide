-- Visual check for per-LOD materials: a row of each faction's T1/T2
-- engineers and T1 tank. Shoot it twice, at the same camera, with LOD0 and
-- with LOD1 forced by the LOD scale:
-- cargo run -- --no-ai --script dev/scenes/visual/lod_materials.lua --camera 400,601.5,14 --screenshot lod0.png --screenshot-tick 30
-- cargo run -- --no-ai --script dev/scenes/visual/lod_materials.lua --camera 400,601.5,14 --screenshot lod1.png --screenshot-tick 30 --ui-lua "ConExecute('cam_SetLOD WorldCamera 4')"
ForkThread(function()
    WaitTicks(2)
    local rows = {
        { 'uel0105', 'uel0208', 'uel0201' },
        { 'ual0105', 'ual0208', 'ual0201' },
        { 'url0105', 'url0208', 'url0107' },
        { 'xsl0105', 'xsl0208', 'xsl0201' },
    }
    for r, row in rows do
        for c, bp in row do
            local x, z = 394 + c * 3, 594 + r * 3
            CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, math.pi, 0)
        end
    end
end)
