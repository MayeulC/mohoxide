-- Pathing stress on a real map: every army's units cross to the start of
-- the army farthest from them (FFA crossing traffic around the map's
-- water and cliffs). Works on any scenario with start markers.
-- Run: ./target/release/rts-core-engine --headless --no-ai --armies 0 \
--        --map /maps/SCMP_009/SCMP_009_scenario.lua --script dev/scenes/stress/crossing.lua \
--        --ticks 600
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
local PER_ARMY = rawget(_G, 'StressPerArmy') or 500
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
    local armies = {}
    for i, brain in ArmyBrains do
        local m = ScenarioUtils.GetMarker(brain.Name)
        if m then table.insert(armies, {name = brain.Name, index = i, pos = m.position}) end
    end
    local total = 0
    for _, a in armies do
        a.units = SpawnArmy(a.name, a.index, a.pos, PER_ARMY)
        total = total + table.getn(a.units)
        WaitTicks(1)
    end
    LOG('stress: ', table.getn(armies), ' armies, ', total, ' units')
    WaitSeconds(2)
    for _, a in armies do
        local far, best = nil, -1
        for _, b in armies do
            local d = VDist2Sq(a.pos[1], a.pos[3], b.pos[1], b.pos[3])
            if d > best then far, best = b, d end
        end
        Order(a.units, far.pos)
    end
    LOG('stress: ordered at tick ', GetGameTick())
end)
