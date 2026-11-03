-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- A formation travels as a block: 16 tanks start in a loose clump and move
-- 60 east in AttackFormation. Members head for their final slots, so the
-- block widens on the way (the real game's 4 x 3 block of uel0201 is 16 of
-- its 21 wide half way, dev/probes probe_landform): half way it's wider
-- than it started and a few rows deep, not a ball or a column; then
-- everyone arrives (the target is by the plateau's edge, so the final
-- layout folds in).
ForkThread(function()
    WaitTicks(2)
    local units = {}
    for i = 0, 15 do
        local x, z = 372 + math.mod(i * 7, 9), 592 + math.mod(i * 5, 13)
        table.insert(units, CreateUnitHPR('uel0201', 'ARMY_1', x, ground(x, z), z, 0, 0, 0))
    end
    for _, u in units do IssueStop({u}) end
    local cmd = IssueFormMove(units, {436, 0, 598}, 'AttackFormation')
    local function spread()
        local minX, maxX, minZ, maxZ, sx = 1e9, -1e9, 1e9, -1e9, 0
        for _, u in units do
            local p = u:GetPosition()
            minX, maxX = math.min(minX, p[1]), math.max(maxX, p[1])
            minZ, maxZ = math.min(minZ, p[3]), math.max(maxZ, p[3])
            sx = sx + p[1]
        end
        return maxX - minX, maxZ - minZ, sx / table.getn(units)
    end
    local _, start = spread()
    local mid = nil
    for _ = 1, 60 do
        WaitTicks(10)
        local depth, width, cx = spread()
        if not mid and cx > 400 then mid = {depth, width, cx} end
        if IsCommandDone(cmd) then break end
    end
    check(mid ~= nil, 'the group got half way')
    if mid then
        note('half way: depth ' .. mid[1] .. ', width ' .. mid[2])
        check(mid[2] > start + 2, 'widening on the move, width ' .. mid[2] .. ' from ' .. start)
        check(mid[1] < 12, 'a few rows deep, depth ' .. mid[1])
    end
    check(IsCommandDone(cmd), 'every unit arrived')
    done()
end)
