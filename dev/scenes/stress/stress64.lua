-- Perf stress driver (docs/guides/running.md, "Stress maps"): every army gets a base
-- and an army, then they all attack-move into their neighbours.
--
--   cargo run --release -- --headless --vault dev/scenes/stress \
--     --map /maps/stress64/stress64_scenario.lua --no-ai \
--     --script dev/scenes/stress/stress64.lua --ticks 3000
--
-- Knobs (edit here, or copy the file): UNITS per army, BASE (structures and
-- factories building), AIR (gunships and fighters in the mix), FIGHT_TICK
-- (when the orders go out).
local UNITS = 60
local BASE = true
local AIR = true
local FIGHT_TICK = 50

local LAND = { 'uel0201', 'ual0201', 'url0107', 'xsl0201', 'uel0103', 'ual0104', 'uel0202', 'url0202', 'xsl0202', 'ual0205' }
local AIRUNITS = { 'uea0203', 'uaa0203', 'uea0102', 'ura0102' }
local ENGINEERS = { 'uel0105', 'ual0105', 'url0105', 'xsl0105' }
local FACTORIES = { 'ueb0101', 'uab0101', 'urb0101', 'xsb0101' }
local MEX = { 'ueb1103', 'uab1103', 'urb1103', 'xsb1103' }
local PGEN = { 'ueb1101', 'uab1101', 'urb1101', 'xsb1101' }
local TANK = { 'uel0201', 'ual0201', 'url0107', 'xsl0201' }

local function ground(x, z) return GetTerrainHeight(x, z) end

local function start_of(army)
    local name = ArmyBrains[army].Name
    local m = Scenario.MasterChain._MASTERCHAIN_.Markers[name]
    return m.position[1], m.position[3]
end

ForkThread(function()
    WaitTicks(1)
    local armies = table.getn(ArmyBrains)
    local spawned, t0 = 0, GetSystemTimeSecondsOnlyForProfileUse and GetSystemTimeSecondsOnlyForProfileUse() or 0
    local groups = {}
    for a = 1, armies do
        local brain = ArmyBrains[a]
        if brain.Name and string.find(brain.Name, '^ARMY_') then
            brain:__engine_SetStorage('MASS', 1e6) brain:__engine_SetStorage('ENERGY', 1e7)
            brain:GiveResource('MASS', 1e6) brain:GiveResource('ENERGY', 1e7)
            local x0, z0 = start_of(a)
            local faction = math.mod(a - 1, 4) + 1
            if BASE then
                for k = 0, 3 do
                    local x, z = x0 - 30 + k * 6, z0 - 30
                    CreateUnitHPR(MEX[faction], a, x, ground(x, z), z, 0, 0, 0)
                    local px, pz = x0 - 30 + k * 6, z0 - 24
                    CreateUnitHPR(PGEN[faction], a, px, ground(px, pz), pz, 0, 0, 0)
                end
                for k = 0, 1 do
                    local fx, fz = x0 - 30 + k * 12, z0 - 12
                    local factory = CreateUnitHPR(FACTORIES[faction], a, fx, ground(fx, fz), fz, 0, 0, 0)
                    IssueBuildFactory({ factory }, TANK[faction], 50)
                    local engs = {}
                    for e = 1, 3 do
                        local ex, ez = fx + e * 2, fz + 8
                        table.insert(engs, CreateUnitHPR(ENGINEERS[faction], a, ex, ground(ex, ez), ez, 0, 0, 0))
                    end
                    IssueGuard(engs, factory)
                end
            end
            local units = {}
            for u = 0, UNITS - 1 do
                local bp
                if AIR and math.mod(u, 6) == 5 then
                    bp = AIRUNITS[math.mod(u, table.getn(AIRUNITS)) + 1]
                else
                    bp = LAND[math.mod(u * 7 + a, table.getn(LAND)) + 1]
                end
                local x, z = x0 + math.mod(u, 10) * 3, z0 + math.floor(u / 10) * 3
                local unit = CreateUnitHPR(bp, a, x, ground(x, z), z, 0, 0, 0)
                if unit then table.insert(units, unit) spawned = spawned + 1 end
            end
            groups[a] = units
        end
    end
    LOG(string.format('STRESS spawned %d units for %d armies', spawned, armies))
    WaitTicks(FIGHT_TICK)
    -- Each army attack-moves into the next one's start (a ring of fronts).
    for a, units in groups do
        local target = math.mod(a, table.getn(groups)) + 1
        local tx, tz = start_of(target)
        IssueAggressiveMove(units, { tx, ground(tx, tz), tz })
    end
    LOG('STRESS orders issued')
    -- Census every 500 ticks: units alive across the armies.
    while true do
        WaitTicks(500)
        local alive = 0
        for a = 1, armies do
            alive = alive + table.getn(ArmyBrains[a]:GetListOfUnits(categories.ALLUNITS, false) or {})
        end
        LOG(string.format('STRESS tick %d units %d', GetGameTick(), alive))
    end
end)
