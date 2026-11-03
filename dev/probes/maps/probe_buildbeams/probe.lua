-- Visual probe: the engineers' build beams. A UEF, Cybran, Aeon and
-- Seraphim T1 engineer (west to east, x 379..409, z 614) each build a land
-- factory 7.5 north of them on Seton's Clutch, as dev/scenes/visual/build_beams.lua does
-- in ours, and a Cybran land factory builds a Mantis at x 424.5. The interface half (ui_probe.lua) points the camera at them,
-- pauses at tick 300 and logs `PROBE shot <name>` for each view, which
-- run_faf.ps1 answers with a screen capture (out\probe_buildbeams_<name>.png).
--   real game (Windows): dev/probes/run_faf.ps1 probe_buildbeams 120
--   ours: cargo run -- --no-ai --script dev/probes/maps/probe_buildbeams/probe.lua
--         --ui-lua "rawset(_G, 'PROBE_NO_PAUSE', true) $(cat dev/probes/maps/probe_buildbeams/ui_probe.lua)"
--         --screenshot x.png --screenshot-tick 300 --speed 3
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local brain = ArmyBrains[1]
brain:GiveStorage('MASS', 50000) brain:GiveStorage('ENERGY', 500000)
brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)

local builds = {{'uel0105', 'ueb0101'}, {'url0105', 'urb0101'}, {'ual0105', 'uab0101'}, {'xsl0105', 'xsb0101'}}
local engineers = {}
for i, p in builds do
    local x = 369 + i * 10
    engineers[i] = {CreateUnitHPR(p[1], armies[1], x, GetTerrainHeight(x, 614), 614, 0, math.pi, 0), p[2], x + 0.5}
end
-- A Cybran land factory east of them building a Mantis.
local factory = CreateUnitHPR('urb0101', armies[1], 424.5, GetTerrainHeight(424.5, 606.5), 606.5, 0, 0, 0)
-- Keep the economy topped up (the real game's start resets what's given here).
ForkThread(function()
    while true do
        brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
        WaitTicks(5)
    end
end)
ForkThread(function()
    WaitTicks(2)
    for _, e in engineers do
        IssueBuildMobile({e[1]}, {e[3], GetTerrainHeight(e[3], 606.5), 606.5}, e[2], {})
    end
    IssueBuildFactory({factory}, 'url0107', 5)
    WaitTicks(300)
    for i, e in engineers do
        local target = e[1]:GetFocusUnit()
        LOG('PROBE build ' .. i .. ' ' .. e[2] .. ' ' .. (target and string.format('%.2f', target:GetFractionComplete()) or '-'))
    end
end)
