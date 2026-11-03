-- Pathing stress: a dense "city" of structures (a grid of 2x2 power
-- generators on narrow streets, with walled-off streets forcing detours)
-- between two armies, who then march through it to each other's start.
-- Exercises JPS in cluttered space (many forced neighbours) and nav-grid
-- blocking at scale.
-- Run: ./target/release/rts-core-engine --headless --vault dev/vault --no-ai --armies 0 \
--        --map /maps/stress_16/stress_16_scenario.lua --script dev/scenes/stress/city.lua \
--        --ticks 600
local PER_ARMY = rawget(_G, 'StressPerArmy') or 1000
-- City half-size (world units) and block pitch: 2x2 generator + street.
local HALF, PITCH = 200, 5
-- Every WALL_EVERY-th street is walled, with a gap every GAP cells.
local WALL_EVERY, GAP = 4, 60
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

-- Helpers duplicated from headon.lua so each stress script stands alone.
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
function FarthestPair()
    local spots = {}
    for i, brain in ArmyBrains do
        local m = ScenarioUtils.GetMarker(brain.Name)
        if m then table.insert(spots, {name = brain.Name, index = i, pos = m.position}) end
    end
    local best, a, b = -1, nil, nil
    for i = 1, table.getn(spots) do
        for j = i + 1, table.getn(spots) do
            local d = VDist2Sq(spots[i].pos[1], spots[i].pos[3], spots[j].pos[1], spots[j].pos[3])
            if d > best then best, a, b = d, spots[i], spots[j] end
        end
    end
    return a, b
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
    local a, b = FarthestPair()
    local cx = math.floor((a.pos[1] + b.pos[1]) / 2)
    local cz = math.floor((a.pos[3] + b.pos[3]) / 2)
    local buildings, walls = 0, 0
    -- One row of blocks per tick (keeps spawn ticks short).
    local row = 0
    for z = cz - HALF, cz + HALF, PITCH do
        for x = cx - HALF, cx + HALF, PITCH do
            if CreateUnitHPR('ueb1101', a.name, x, GetTerrainHeight(x, z), z, 0, 0, 0) then
                buildings = buildings + 1
            end
        end
        -- Wall the street north of this row, leaving gaps.
        if math.mod(row, WALL_EVERY) == WALL_EVERY - 1 then
            local wz = z + 2.5
            for x = cx - HALF, cx + HALF + PITCH do
                if math.mod(x - (cx - HALF) + row * 17, GAP) >= 4 then
                    if CreateUnitHPR('ueb5101', a.name, x + 0.5, GetTerrainHeight(x, wz), wz, 0, 0, 0) then
                        walls = walls + 1
                    end
                end
            end
        end
        row = row + 1
        WaitTicks(1)
    end
    local ua = SpawnArmy(a.name, a.index, a.pos, PER_ARMY)
    WaitTicks(1)
    local ub = SpawnArmy(b.name, b.index, b.pos, PER_ARMY)
    LOG('stress: city of ', buildings, ' generators and ', walls, ' walls at ', cx, ',', cz,
        '; ', table.getn(ua) + table.getn(ub), ' units')
    WaitSeconds(2)
    -- Through the city: each army heads for the far side of it, then on.
    Order(ua, b.pos)
    Order(ub, a.pos)
    LOG('stress: ordered at tick ', GetGameTick())
end)
