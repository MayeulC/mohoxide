-- Fatboys built on sloping ground, Seton's Clutch: on the right
-- (128, 830; the ground under it varies by 3.8) one built fast, finished by
-- tick ~70; on the left (80, 830; 1.7) one still being built. Moho leaves a
-- construction level at its centre's height, then stands it on the ground
-- the tick it's finished (dev/probes/maps/probe_fatboy).
-- cargo run -- --no-ai --script dev/scenes/visual/fatboy_slope.lua --camera 104,830,90 --screenshot shot.png --screenshot-tick 150
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 500000) b:__engine_SetStorage('ENERGY', 5000000)
    b:GiveResource('MASS', 500000) b:GiveResource('ENERGY', 5000000)
    local function at(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local fast, slow = at('uel0309', 136, 830), at('uel0309', 88, 830)
    IssueBuildMobile({ fast }, { 128, GetTerrainHeight(128, 830), 830 }, 'uel0401', {})
    IssueBuildMobile({ slow }, { 80, GetTerrainHeight(80, 830), 830 }, 'uel0401', {})
    while true do
        WaitTicks(1)
        fast:SetBuildRate(4000)
        slow:SetBuildRate(400)
    end
end)
