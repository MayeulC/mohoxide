-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- Units on the shore cliffs west of Seton's middle (200, 616 drops east
-- into the sea) are sent out to sea and back: 36 tanks on a plain
-- move (a flow field), 36 amphibious units and 36 hovercraft in formation.
-- No unit may stand in a cell its class can't, by moho's rule
-- (`OCCUPY_MobileCheck`): ground layers need every corner edge rising at
-- most 0.75 and the deepest corner at most `MaxWaterDepth` under water
-- (tanks 0.05, amphibious 25, hover 1); water layers the shallowest corner
-- at least `MinWaterDepth` under (hover 0.1). Before, steering cut path
-- corners and tanks drove over the cliffs into the sea, and hovercraft
-- crossed cliff cells whose mean depth, not every corner, was 0.1.
local water = GetSurfaceHeight(0, 0)
local function cell(x, z)
    local cx, cz = math.floor(x), math.floor(z)
    local h00, h10 = ground(cx, cz), ground(cx + 1, cz)
    local h01, h11 = ground(cx, cz + 1), ground(cx + 1, cz + 1)
    local step = math.max(math.abs(h00 - h10), math.abs(h00 - h01), math.abs(h10 - h11), math.abs(h01 - h11))
    return step, water - math.max(h00, h10, h01, h11), water - math.min(h00, h10, h01, h11)
end
local rules = { uel0201 = {max = 0.05}, url0203 = {max = 25}, uel0203 = {max = 1, min = 0.1} }
local function fits(id, x, z)
    local r = rules[id]
    local step, shallowest, deepest = cell(x, z)
    return (step <= 0.75 and deepest <= r.max) or (r.min ~= nil and shallowest >= r.min), step, shallowest, deepest
end
ForkThread(function()
    WaitTicks(2)
    local function group(bp, x0, z0)
        local g = {}
        for i = 0, 35 do
            local x, z = x0 - math.mod(i, 6) * 2.5, z0 + math.floor(i / 6) * 2.5
            table.insert(g, CreateUnitHPR(bp, 'ARMY_1', x, ground(x, z), z, 0, math.pi / 2, 0))
        end
        return g
    end
    local tanks, amphibious, hover = group('uel0201', 185, 590), group('url0203', 185, 625), group('uel0203', 170, 605)
    local all = {}
    for _, g in {tanks, amphibious, hover} do
        for _, u in g do table.insert(all, u) end
    end
    IssueMove(tanks, {300, 0, 616})
    IssueFormMove(amphibious, {300, 0, 616}, 'AttackFormation', 0)
    IssueFormMove(hover, {300, 0, 600}, 'AttackFormation', 0)
    local bad, first = 0, nil
    for t = 1, 1300 do
        WaitTicks(1)
        if t == 600 then
            IssueClearCommands(all)
            IssueMove(tanks, {150, 0, 600})
            IssueFormMove(amphibious, {150, 0, 630}, 'AttackFormation', 0)
            IssueFormMove(hover, {160, 0, 615}, 'AttackFormation', 0)
        end
        for _, u in all do
            local p = u:GetPosition()
            local ok, step, shallowest, deepest = fits(u.UnitId, p[1], p[3])
            if not ok then
                bad = bad + 1
                first = first or string.format('tick %d: %s at %.1f, %.1f (step %.2f, depth %.2f..%.2f)', t, u.UnitId, p[1], p[3], step, shallowest, deepest)
            end
        end
    end
    note('unit-ticks on closed cells: ' .. bad)
    check(bad == 0, 'units on cells their class cannot stand on: ' .. bad .. ', first ' .. tostring(first))
    local p = tanks[1]:GetPosition()
    check(VDist2(p[1], p[3], 150, 600) < 15, string.format('tanks came back up, first at %.1f, %.1f', p[1], p[3]))
    done()
end)
