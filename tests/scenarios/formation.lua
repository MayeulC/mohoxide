-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    return b
end
-- IssueFormMove lays 12 tanks and 4 scouts out with FAF's AttackFormation
-- facing east: two rows across z, spread out, every unit arrives.
ForkThread(function()
    WaitTicks(2)
    local units = {}
    for i = 0, 11 do
        local x, z = 380 + math.mod(i, 4) * 3, 560 + math.floor(i / 4) * 3
        table.insert(units, CreateUnitHPR('uel0201', 'ARMY_1', x, ground(x, z), z, 0, 0, 0))
    end
    for i = 0, 3 do table.insert(units, CreateUnitHPR('uel0101', 'ARMY_1', 380 + i * 3, ground(380 + i * 3, 572), 572, 0, 0, 0)) end
    for _, u in units do IssueStop({u}) end
    local cmd = IssueFormMove(units, {420, 0, 600}, 'AttackFormation', 90)
    for _ = 1, 40 do
        WaitTicks(10)
        if IsCommandDone(cmd) then break end
    end
    check(IsCommandDone(cmd), 'every unit finished the formation move')
    local minX, maxX, minZ, maxZ, minD = 1e9, -1e9, 1e9, -1e9, 1e9
    for i, u in units do
        local p = u:GetPosition()
        minX, maxX = math.min(minX, p[1]), math.max(maxX, p[1])
        minZ, maxZ = math.min(minZ, p[3]), math.max(maxZ, p[3])
        for j = i + 1, table.getn(units) do
            local q = units[j]:GetPosition()
            minD = math.min(minD, VDist2(p[1], p[3], q[1], q[3]))
        end
    end
    -- The real game: width 20.6, depth 4.8, closest pair 2.64 (probe_formation).
    check(maxZ - minZ > 18, 'spread across the heading (z), width ' .. (maxZ - minZ))
    check(maxX - minX < 12, 'a few rows deep along it (x), depth ' .. (maxX - minX))
    check(minD > 1, 'nobody stacked, closest pair ' .. minD)
    done()
end)
