-- Development scene for the engineers' build beams, on the land patch
-- of scmp_009: a UEF, Cybran, Aeon and Seraphim T1 engineer (west to east)
-- each building a land factory 7 units north of it, so the beams run for a
-- minute. The Seraphim pair is at x 409.
-- Run: cargo run -- --no-ai --script dev/scenes/visual/build_beams.lua --camera 394,610,36
-- (close on the Seraphim one: --camera 409,610,12)
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end
local brain = ArmyBrains[1]
brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)

local builds = {{'uel0105', 'ueb0101'}, {'url0105', 'urb0101'}, {'ual0105', 'uab0101'}, {'xsl0105', 'xsb0101'}}
local engineers = {}
for i, p in builds do
    local x = 369 + i * 10
    engineers[i] = {spawn(p[1], 1, x, 614, math.pi), p[2], x + 0.5}
end
ForkThread(function()
    WaitTicks(2)
    for _, e in engineers do
        IssueBuildMobile({e[1]}, {e[3], GetTerrainHeight(e[3], 606.5), 606.5}, e[2], {})
    end
end)
