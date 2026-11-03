-- Visual check for the graphics pass: every faction's power generators (T1-T3)
-- and shield generators (shields up), a mass extractor, radar and a skirmish.
-- cargo run -- --no-ai --script dev/scenes/visual/graphics_showcase.lua --camera 410,600,60
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local rows = {
        { 'ueb1101', 'ueb1201', 'ueb1301', 'ueb4202', 'ueb4301' },
        { 'uab1101', 'uab1201', 'uab1301', 'uab4202', 'uab4301' },
        { 'urb1101', 'urb1201', 'urb1301', 'urb4202', 'urb4204' },
        { 'xsb1101', 'xsb1201', 'xsb1301', 'xsb4202', 'xsb4301' },
    }
    for r, row in rows do
        for c, bp in row do
            at(bp, 'ARMY_1', 380 + c * 14, 570 + r * 14)
        end
    end
    at('ueb1103', 'ARMY_1', 380, 640)
    at('ueb3101', 'ARMY_1', 394, 640)
    for i = 1, 3 do
        at('uel0201', 'ARMY_1', 450 + i * 3, 600)
        at('ual0201', 'ARMY_2', 470 + i * 3, 600)
    end
end)
