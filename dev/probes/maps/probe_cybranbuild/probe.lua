-- Visual probe: Cybran structures under construction. Four Cybran T1
-- engineers each build a T1 power generator (urb1101, 250 ticks at their
-- build rate) on Seton's Clutch's open grass, starting at different ticks so
-- the four stand at different fractions when the interface half pauses at
-- tick 300 (roughly 0.3, 0.55, 0.85 and 0.95+: under 70% the technique is 40%
-- visible, then it ramps to opaque; the team colour comes in over the last
-- 10%, the red overlay fades over the last 5%). The sim logs each one's
-- fraction around the pause as `PROBE fraction <tick> <i> <fraction>`.
--   real game (Windows): dev/probes/run_faf.ps1 probe_cybranbuild 90
--   ours: cargo run -- --no-ai --script dev/probes/maps/probe_cybranbuild/probe.lua
--         --ui-lua "rawset(_G, 'PROBE_NO_PAUSE', true) $(cat dev/probes/maps/probe_cybranbuild/ui_probe.lua)"
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

-- Build sites (x, z) and the tick each engineer starts.
local sites = { {383, 606, 60}, {390, 606, 85}, {383, 613, 160}, {390, 613, 225} }
local engineers = {}
for i, s in sites do
    local x, z = s[1], s[2] + 3.5
    engineers[i] = CreateUnitHPR('url0105', armies[1], x, GetTerrainHeight(x, z), z, 0, math.pi, 0)
end
ForkThread(function()
    while true do
        brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
        WaitTicks(5)
    end
end)
-- (The site goes in as an argument: FA's Lua 5.0 closures don't keep a for
-- loop's variables per iteration.)
for i, s in sites do
    ForkThread(function(site, engineer)
        WaitTicks(site[3])
        IssueBuildMobile({engineer}, {site[1], GetTerrainHeight(site[1], site[2]), site[2]}, 'urb1101', {})
    end, s, engineers[i])
end
ForkThread(function()
    WaitTicks(296)
    for _ = 1, 10 do
        for i, e in engineers do
            local target = not e:BeenDestroyed() and e:GetFocusUnit()
            LOG('PROBE fraction ' .. GetGameTick() .. ' ' .. i .. ' ' .. (target and string.format('%.3f', target:GetFractionComplete()) or '-'))
        end
        WaitTicks(1)
    end
end)
