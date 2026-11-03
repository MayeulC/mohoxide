-- Land stress, free for all: PER_ARMY land units for every army (its
-- faction's T1 / T2 / T3 tanks, plus an experimental per PER_EXPERIMENTAL),
-- every army an enemy of every other, then everyone attack-moves on the map
-- centre: 63 armies x (1000 + 10) is ~64k units in one brawl. Armies spawn
-- one per tick, and every army sees the whole map. The land counterpart of
-- air.lua.
-- Run (windowed):
--   cargo run --release -- --no-audio --vault dev/vault --no-ai --armies 0 --player 1 \
--     --map /maps/stress_63/stress_63_scenario.lua --script dev/scenes/stress/land.lua
-- Headless with a Tracy capture: see docs/guides/running.md "Profiling with Tracy".
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
local PER_ARMY = rawget(_G, 'StressPerArmy') or 1000
local PER_EXPERIMENTAL = rawget(_G, 'StressPerExperimental') or 100
-- Every army sees the whole map (vision, radar, omni), so targeting isn't
-- limited by intel. StressFullVision = false keeps normal intel.
local FULL_VISION = rawget(_G, 'StressFullVision') ~= false

-- Army i fields faction (i - 1) % 4.
local FACTIONS = {
    {units = {'uel0201', 'uel0202', 'uel0303'}, experimental = 'uel0401'}, -- Fatboy
    {units = {'ual0201', 'ual0202', 'ual0303'}, experimental = 'ual0401'}, -- Galactic Colossus
    {units = {'url0107', 'url0202', 'url0303'}, experimental = 'url0402'}, -- Monkeylord
    {units = {'xsl0201', 'xsl0202', 'xsl0303'}, experimental = 'xsl0401'}, -- Ythotha
}

-- `n` units in a square block around `at`, its experimentals in a row behind.
function SpawnLandArmy(army, index, at, n)
    local f = FACTIONS[math.mod(index - 1, 4) + 1]
    local units = {}
    local cols = math.ceil(math.sqrt(n))
    local function spawn(bp, x, z)
        local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
        if u then table.insert(units, u) end
    end
    for k = 0, n - 1 do
        spawn(f.units[math.mod(k, 3) + 1], at[1] + (math.mod(k, cols) - cols / 2) * 2.5, at[3] + (math.floor(k / cols) - cols / 2) * 2.5)
    end
    local exps = math.floor(n / PER_EXPERIMENTAL)
    for k = 0, exps - 1 do
        spawn(f.experimental, at[1] + (k - (exps - 1) / 2) * 14, at[3] + cols / 2 * 2.5 + 14)
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
            SetArmyUnitCap(i, PER_ARMY + PER_ARMY / PER_EXPERIMENTAL + 100)
            table.insert(armies, {name = brain.Name, index = i, start = marker.position})
        end
    end
    -- Free for all whatever --teams says.
    for _, a in armies do
        for _, b in armies do
            if a.index ~= b.index then SetAlliance(a.index, b.index, 'Enemy') end
        end
    end
    if FULL_VISION then
        local ScenarioFramework = import('/lua/ScenarioFramework.lua')
        local radius = math.sqrt(w * w + h * h) / 2 + 10
        for _, a in armies do
            ScenarioFramework.CreateVisibleAreaLocation(radius, {cx, 0, cz}, 0, ArmyBrains[a.index])
        end
    end
    local total = 0
    for _, a in armies do
        a.units = SpawnLandArmy(a.name, a.index, a.start, PER_ARMY)
        total = total + table.getn(a.units)
        WaitTicks(1)
    end
    LOG('land stress: ', table.getn(armies), ' armies (free for all), ', total, ' units spawned by tick ', GetGameTick())
    WaitSeconds(2)
    for _, a in armies do
        IssueFormAggressiveMove(a.units, {cx, GetTerrainHeight(cx, cz), cz}, 'AttackFormation')
    end
    LOG('land stress: everyone ordered to the centre at tick ', GetGameTick())
    while true do
        WaitSeconds(10)
        local alive, standing = 0, 0
        for _, a in armies do
            local n = 0
            for _, u in a.units do if not u.Dead then n = n + 1 end end
            alive = alive + n
            if n > 0 then standing = standing + 1 end
        end
        LOG('land stress: tick ', GetGameTick(), ' units alive ', alive, ', armies standing ', standing)
    end
end)
