-- Development scenario: a Czar (army 1) over enemy tanks, and a Seraphim
-- T2 point defence (army 1) beside a row of enemy tanks; both beams should run
-- along the beam, not sideways.
-- Run: cargo run -- --script dev/scenes/visual/czar_beam.lua --no-ai --player 1 --camera 512,512,90 --no-audio
local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local function spawn(bp, army, x, z, heading)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, heading, 0, 0)
    LOG('dev/czar_beam: ', bp, ' army ', army, ' entity ', u:GetEntityId())
    return u
end
spawn('uaa0310', 1, cx - 20, cz, 0)
for row = -1, 1 do
    for col = -1, 1 do
        spawn('uel0201', 2, cx - 20 + col * 5, cz + row * 5, 0)
    end
end
spawn('xsb2301', 1, cx + 20, cz, 0)
for row = -1, 1 do
    spawn("uel0201", 2, cx + 30, cz + row * 6, -math.pi / 2)
end
