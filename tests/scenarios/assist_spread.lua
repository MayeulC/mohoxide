-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- Engineers sent one by one (an order each, as FAF's AI sends them) and
-- then three together to assist a factory from the same side all park at
-- the same spot of its skirt. They should spread out there, not stack on
-- one point, settle (no driving back onto the pile) and all work.
ForkThread(function()
    WaitTicks(2)
    local factory = CreateUnitHPR('ueb0101', 'ARMY_1', 420.5, ground(420.5, 600.5), 600.5, 0, 0, 0)
    IssueBuildFactory({factory}, 'uel0201', 20)
    local engineers = {}
    for i = 1, 6 do
        local x, z = 450, 598 + (i % 2)
        local e = CreateUnitHPR('uel0105', 'ARMY_1', x, ground(x, z), z, 0, 0, 0)
        table.insert(engineers, e)
    end
    IssueStop(engineers)
    -- (Let the new engineers slide off each other first.)
    WaitTicks(10)
    for i = 1, 3 do
        IssueGuard({engineers[i]}, factory)
        WaitTicks(15)
    end
    IssueGuard({engineers[4], engineers[5], engineers[6]}, factory)
    WaitTicks(250)
    local closest, moving, working = 1000, 0, 0
    for i, a in ipairs(engineers) do
        if a:IsUnitState('Moving') then moving = moving + 1 end
        if a:GetFocusUnit() then working = working + 1 end
        local pa = a:GetPosition()
        for j = i + 1, table.getn(engineers) do
            local pb = engineers[j]:GetPosition()
            closest = math.min(closest, VDist2(pa[1], pa[3], pb[1], pb[3]))
        end
    end
    check(closest >= 0.8, 'engineers stacked on one spot: closest pair ' .. closest .. ' apart')
    check(moving == 0, moving .. ' engineers still moving (driving back onto the pile?)')
    check(working == 6, 'only ' .. working .. ' of 6 engineers assisting')
    note('closest pair ' .. closest .. ', moving ' .. moving .. ', working ' .. working)
    done()
end)
