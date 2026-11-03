-- Development scene for the Seraphim build-in effect, on the land
-- patch of scmp_009: five Seraphim engineers each building a land factory,
-- started 120 ticks apart, so at tick ~600 the row (west to east) is about
-- 95%, 75%, 55%, 35% and 15% built; a finished factory stands at the west
-- end. Fractions are logged as `SERAPHIM BUILD` lines every 20 ticks.
-- Run: cargo run -- --no-ai --script dev/scenes/visual/seraphim_build.lua --camera 390,600,44 --screenshot s.png --screenshot-tick 600
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end
local brain = ArmyBrains[1]
brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)

local sites = {}
for i = 1, 5 do
    local x = 366 + i * 8
    sites[i] = {spawn('xsl0105', 1, x, 614), x + 0.5}
end
spawn('xsb0101', 1, 366.5, 604.5)
ForkThread(function()
    for i, s in sites do
        WaitTicks(i == 1 and 2 or 120)
        IssueBuildMobile({s[1]}, {s[2], GetTerrainHeight(s[2], 604.5), 604.5}, 'xsb0101', {})
    end
end)
ForkThread(function()
    while true do
        WaitTicks(20)
        local parts = {}
        for i, s in sites do
            local target = s[1]:GetFocusUnit()
            parts[i] = target and string.format('%.2f', target:GetFractionComplete()) or '-'
        end
        LOG('SERAPHIM BUILD tick ' .. GetGameTick() .. ': ' .. table.concat(parts, ' '))
    end
end)
