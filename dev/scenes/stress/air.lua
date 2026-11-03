-- Air stress: PER_ARMY fighters for every army (its faction's T1
-- interceptors, T2 fighter-bombers and T3 air superiority fighters), then
-- everyone attack-moves on the map centre: one huge dogfight. Armies spawn
-- one per tick, in the air.
-- Run (windowed):
--   cargo run --release -- --vault dev/vault --no-ai --armies 0 --player 1 \
--     --map /maps/stress_63/stress_63_scenario.lua --script dev/scenes/stress/air.lua
-- Headless with a Tracy capture: see docs/guides/running.md "Profiling with Tracy".
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
local PER_ARMY = rawget(_G, 'StressPerArmy') or 1000

-- Interceptor / fighter-bomber / ASF per faction (army i flies faction (i - 1) % 4).
local FACTIONS = {
    {'uea0102', 'dea0202', 'uea0303'},
    {'uaa0102', 'xaa0202', 'uaa0303'},
    {'ura0102', 'dra0202', 'ura0303'},
    {'xsa0102', 'xsa0202', 'xsa0303'},
}

function SpawnAirArmy(army, index, at, n)
    local f = FACTIONS[math.mod(index - 1, 4) + 1]
    local units = {}
    local cols = math.ceil(math.sqrt(n))
    for k = 0, n - 1 do
        local x = at[1] + (math.mod(k, cols) - cols / 2) * 3
        local z = at[3] + (math.floor(k / cols) - cols / 2) * 3
        local u = CreateUnitHPR(f[math.mod(k, 3) + 1], army, x, GetTerrainHeight(x, z) + 20, z, 0, 0, 0)
        if u then table.insert(units, u) end
    end
    return units
end

ForkThread(function()
    WaitTicks(1)
    local w, h = GetMapSize()
    local cx, cz = w / 2, h / 2
    local armies = {}
    for i, brain in ArmyBrains do
        local marker = ScenarioUtils.GetMarker(brain.Name)
        if marker then
            SetArmyUnitCap(i, PER_ARMY + 100)
            table.insert(armies, {name = brain.Name, index = i, start = marker.position})
        end
    end
    local total = 0
    for _, a in armies do
        a.units = SpawnAirArmy(a.name, a.index, a.start, PER_ARMY)
        total = total + table.getn(a.units)
        WaitTicks(1)
    end
    LOG('air stress: ', table.getn(armies), ' armies, ', total, ' aircraft spawned by tick ', GetGameTick())
    WaitSeconds(2)
    for _, a in armies do
        IssueAggressiveMove(a.units, {cx, GetTerrainHeight(cx, cz), cz})
    end
    LOG('air stress: everyone ordered to the centre at tick ', GetGameTick())
    while true do
        WaitSeconds(10)
        local alive = 0
        for _, a in armies do
            for _, u in a.units do if not u.Dead then alive = alive + 1 end end
        end
        LOG('air stress: tick ', GetGameTick(), ' aircraft alive ', alive)
    end
end)
