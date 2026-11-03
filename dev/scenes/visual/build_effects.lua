-- Development scene for build effects, on the land patch of scmp_009: a UEF
-- ACU building an air factory (slice beams from both hands, the build
-- cube rising), a UEF engineer building a mass storage, and a land factory
-- building a tank; Cybran, Aeon and Seraphim engineers building power
-- generators (camera 408,612,20).
-- Run: cargo run -- --script dev/scenes/visual/build_effects.lua --camera 412,598,30
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end
local brain = ArmyBrains[1]
brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)

local acu = spawn('uel0001', 1, 404, 598)
local eng = spawn('uel0105', 1, 420, 604)
local fac = spawn('ueb0101', 1, 420, 590)
-- The other factions' build-in looks, side by side south of the UEF ones.
local others = {}
for i, b in {{'url0105', 'urb1101'}, {'ual0105', 'uab1101'}, {'xsl0105', 'xsb1101'}} do
    local x = 396 + i * 6
    others[i] = {spawn(b[1], 1, x, 616), b[2], x}
end
ForkThread(function()
    WaitTicks(5)
    IssueBuildMobile({acu}, {410.5, GetTerrainHeight(410.5, 602.5), 602.5}, 'ueb0102', {})
    IssueBuildMobile({eng}, {416.5, GetTerrainHeight(416.5, 606.5), 606.5}, 'ueb1106', {})
    IssueBuildFactory({fac}, 'uel0201', 5)
    for _, o in others do
        IssueBuildMobile({o[1]}, {o[3] + 0.5, GetTerrainHeight(o[3] + 0.5, 612.5), 612.5}, o[2], {})
    end
    while true do
        WaitTicks(20)
        local n = 0
        for _ in acu.BuildEffectsBag or {} do n = n + 1 end
        local p = acu.UEFBuildProjectile
        LOG('build effects: acu bag ' .. n .. ', building ' .. tostring(acu:IsUnitState('Building')) .. ', beam end ' .. tostring(p and not p:BeenDestroyed()) .. ' at ' .. (p and not p:BeenDestroyed() and string.format('%.2f %.2f %.2f', unpack(p:GetPosition())) or '-'))
    end
end)
