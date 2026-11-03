-- Scenario test (tests/scenarios.rs): a UEF quantum gateway blocks only its
-- two OccupyRects pads for pathing, so a tank sent across it drives straight
-- through the lane between them instead of around.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    CreateUnitHPR('ueb0304', 'ARMY_1', 400.5, GetTerrainHeight(400.5, 570.5), 570.5, 0, 0, 0)
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400.5, GetTerrainHeight(400.5, 592), 592, 0, 0, 0)
    IssueMove({tank}, {400.5, 0, 548})
    local off = 0
    for _ = 1, 200 do
        WaitTicks(2)
        local p = tank:GetPosition()
        off = math.max(off, math.abs(p[1] - 400.5))
        if VDist2(p[1], p[3], 400.5, 548) < 1 then break end
    end
    local p = tank:GetPosition()
    note(string.format('widest %.1f off the line, ended %.1f, %.1f', off, p[1], p[3]))
    check(VDist2(p[1], p[3], 400.5, 548) < 1, 'arrived')
    check(off < 1.5, 'through the lane between the pads (widest ' .. off .. ')')
    ScenarioResult.done = true
end)
