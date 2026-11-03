-- Development scenario: a Cybran ACU (army 1) with the Microwave Laser
-- Generator enhancement, facing a line of enemy tanks (army 2) in range.
-- Run: cargo run -- --script dev/scenes/visual/cybran_acu_laser.lua --no-ai --player 1 --camera 516,512,50
local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local acu = CreateUnitHPR('url0001', 1, cx, GetTerrainHeight(cx, cz), cz, math.pi / 2, 0, 0)
acu:CreateEnhancement('MicrowaveLaserGenerator')
LOG('dev/cybran_acu_laser: enhancement ', tostring(acu:HasEnhancement('MicrowaveLaserGenerator')))
for row = -2, 2 do
    CreateUnitHPR('uel0201', 2, cx + 14, GetTerrainHeight(cx + 14, cz + row * 5), cz + row * 5, -math.pi / 2, 0, 0)
end
