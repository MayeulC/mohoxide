-- Air duel: PER_ARMY fighters each for two armies (their factions' T1
-- interceptors, T2 fighter-bombers and T3 air superiority fighters). The
-- player's army (StressPlayerArmy, default 1: pass the same --player) is
-- left to you; the other attack-moves on your start. Each side spawns BATCH
-- a tick, and both see the whole map (StressFullVision = false: normal intel).
-- Run (windowed, you fly army 1):
--   cargo run --release -- --no-audio --no-ai --armies 2 --player 1 \
--     --map /maps/SCMP_009/SCMP_009_scenario.lua --script dev/scenes/stress/air_duel.lua
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
local PER_ARMY = rawget(_G, 'StressPerArmy') or 10000
local PLAYER = rawget(_G, 'StressPlayerArmy') or 1
local BATCH = 1000
local FULL_VISION = rawget(_G, 'StressFullVision') ~= false

-- Interceptor / fighter-bomber / ASF per faction (army i flies faction (i - 1) % 4).
local FACTIONS = {
    {'uea0102', 'dea0202', 'uea0303'},
    {'uaa0102', 'xaa0202', 'uaa0303'},
    {'ura0102', 'dra0202', 'ura0303'},
    {'xsa0102', 'xsa0202', 'xsa0303'},
}

ForkThread(function()
    WaitTicks(1)
    local w, h = GetMapSize()
    local cx, cz = w / 2, h / 2
    local armies = {}
    for i, brain in ArmyBrains do
        local marker = ScenarioUtils.GetMarker(brain.Name)
        if marker then
            SetArmyUnitCap(i, PER_ARMY + 100)
            table.insert(armies, {name = brain.Name, index = i, start = marker.position, units = {},
                human = i == PLAYER})
        end
    end
    if FULL_VISION then
        local ScenarioFramework = import('/lua/ScenarioFramework.lua')
        local radius = math.sqrt(w * w + h * h) / 2 + 10
        for _, a in armies do
            ScenarioFramework.CreateVisibleAreaLocation(radius, {cx, 0, cz}, 0, ArmyBrains[a.index])
        end
    end
    -- Spawn both sides a batch a tick, each in a square block over its start.
    local cols = math.ceil(math.sqrt(PER_ARMY))
    for first = 0, PER_ARMY - 1, BATCH do
        for _, a in armies do
            local f = FACTIONS[math.mod(a.index - 1, 4) + 1]
            for k = first, math.min(first + BATCH, PER_ARMY) - 1 do
                local x = a.start[1] + (math.mod(k, cols) - cols / 2) * 3
                local z = a.start[3] + (math.floor(k / cols) - cols / 2) * 3
                local u = CreateUnitHPR(f[math.mod(k, 3) + 1], a.name, x, GetTerrainHeight(x, z) + 20, z, 0, 0, 0)
                if u then table.insert(a.units, u) end
            end
        end
        WaitTicks(1)
    end
    for _, a in armies do
        LOG('air duel: ', a.name, (a.human and ' (you)' or ''), ' has ', table.getn(a.units), ' aircraft')
    end
    WaitSeconds(2)
    local player
    for _, a in armies do if a.human then player = a end end
    for _, a in armies do
        if not a.human then
            local target = player and player.start or {cx, 0, cz}  -- no player army: meet in the middle
            IssueAggressiveMove(a.units, {target[1], GetTerrainHeight(target[1], target[3]), target[3]})
        end
    end
    LOG('air duel: attack ordered at tick ', GetGameTick())
    while true do
        WaitSeconds(10)
        local line = ''
        for _, a in armies do
            local n = 0
            for _, u in a.units do if not u.Dead then n = n + 1 end end
            line = line .. ' ' .. a.name .. ' ' .. n
        end
        LOG('air duel: tick ', GetGameTick(), line)
    end
end)
