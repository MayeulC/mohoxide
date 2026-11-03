-- Stress test: PER_ARMY land units (plus experimentals) for every army at its start spot, then
-- everyone marches on the map centre (aggressive move, in formation) and fights. Armies
-- spawn one per tick.
-- Maps: dev/scenes/stress/gen_stress_map.py <armies> (into dev/vault).
-- Run: cargo run --release -- --headless --vault dev/vault --no-ai --armies 0 \
--        --map /maps/stress_16/stress_16_scenario.lua --script dev/scenes/stress/stress.lua \
--        --ticks 600
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
local PER_ARMY = rawget(_G, 'StressPerArmy') or 1000
-- T1 / T2 / T3 land units and the experimental per faction (army i fields
-- faction (i - 1) % 4).
local FACTIONS = {
    {units = {'uel0201', 'uel0202', 'uel0303'}, experimental = 'uel0401'}, -- Fatboy
    {units = {'ual0201', 'ual0202', 'ual0303'}, experimental = 'ual0401'}, -- Galactic Colossus
    {units = {'url0107', 'url0202', 'url0303'}, experimental = 'url0402'}, -- Monkeylord
    {units = {'xsl0201', 'xsl0202', 'xsl0303'}, experimental = 'xsl0401'}, -- Ythotha
}
-- One experimental per this many units, in a row behind the block.
local PER_EXPERIMENTAL = rawget(_G, 'StressPerExperimental') or 100

-- Orders go in formation, as players give them (StressFormation = 'none'
-- for plain orders: every unit to the same point).
local FORMATION = rawget(_G, 'StressFormation') or 'AttackFormation'
local function Order(units, pos, aggressive)
    if FORMATION == 'none' then
        if aggressive then IssueAggressiveMove(units, pos) else IssueMove(units, pos) end
    elseif aggressive then
        IssueFormAggressiveMove(units, pos, FORMATION)
    else
        IssueFormMove(units, pos, FORMATION)
    end
end

-- `n` units of army `index`'s faction in a square block around `at`, plus
-- its experimentals in a row beside it.
function SpawnArmy(army, index, at, n)
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
            table.insert(armies, {name = brain.Name, index = i, start = marker.position, units = {}})
        end
    end
    local total = 0
    for _, a in armies do
        a.units = SpawnArmy(a.name, a.index, a.start, PER_ARMY)
        total = total + table.getn(a.units)
        -- An army per tick: one giant spawn tick trips the stall watchdog
        -- at 128 armies, and per-tick spawn cost shows in the profile.
        WaitTicks(1)
    end
    LOG('stress: ', table.getn(armies), ' armies, ', total, ' units spawned by tick ', GetGameTick())
    WaitSeconds(2)
    for _, a in armies do
        Order(a.units, {cx, GetTerrainHeight(cx, cz), cz}, true)
    end
    LOG('stress: armies ordered to the centre at tick ', GetGameTick())
end)
