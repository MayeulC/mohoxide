-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- A plain move given to 40 tanks (at least movement::FLOW_MIN) follows a
-- shared flow field: everyone gets there and gathers round the point.
ForkThread(function()
    WaitTicks(2)
    local units = {}
    for i = 0, 39 do
        local x, z = 372 + math.mod(i, 8) * 2.5, 588 + math.floor(i / 8) * 2.5
        table.insert(units, CreateUnitHPR('uel0201', 'ARMY_1', x, ground(x, z), z, 0, 0, 0))
    end
    for _, u in units do IssueStop({u}) end
    local cmd = IssueMove(units, {432, 0, 604})
    for _ = 1, 60 do
        WaitTicks(10)
        if IsCommandDone(cmd) then break end
    end
    check(IsCommandDone(cmd), 'everyone arrived')
    local far = 0
    for _, u in units do
        local p = u:GetPosition()
        far = math.max(far, VDist2(p[1], p[3], 432, 604))
    end
    note('farthest from the point: ' .. far)
    check(far < 15, 'gathered round the point, farthest ' .. far)
    done()
end)
