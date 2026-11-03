-- Scenario test (tests/scenarios.rs): aircraft fly moho's attitude
-- controller (KTurn / KRoll, BankFactor). A UEF T1 interceptor flying north
-- turned hard right banks right wing down and levels out once it's on its
-- new course; a gunship sent sideways leans into it (moho leans hovering
-- aircraft only across the nose unless BankForward).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
-- The y of the unit's right (local +X) and up (local +Y) axes.
local function axes(u)
    local q = u:GetOrientation()
    local x, y, z, w = q[1], q[2], q[3], q[4]
    return 2 * (x * y + w * z), 1 - 2 * (x * x + z * z)
end
ForkThread(function()
    WaitTicks(2)
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 400, GetTerrainHeight(400, 300) + 20, 300, 0, 0, 0)
    IssueMove({plane}, Vector(400, 0, 700))
    WaitTicks(40)
    local straight = select(1, axes(plane))
    IssueClearCommands({plane})
    IssueMove({plane}, Vector(700, 0, 420))
    -- (Made now: idle, it would have landed by this time.)
    local gunship = CreateUnitHPR('uea0203', 'ARMY_1', 300, GetTerrainHeight(300, 300) + 20, 300, 0, 0, 0)
    IssueMove({gunship}, Vector(420, 0, 300))
    local min_right, max_lean, settled = 1, 0, nil
    for t = 1, 150 do
        WaitTicks(1)
        local right = axes(plane)
        if right < min_right then min_right = right end
        local up_y = select(2, axes(gunship))
        if 1 - up_y > max_lean then max_lean = 1 - up_y end
        if t > 100 and settled == nil then settled = right end
    end
    note(string.format('flying straight: right axis y %.3f; turning: lowest %.3f; on course: %.3f; gunship lean 1 - up.y %.4f',
        straight, min_right, settled or 0, max_lean))
    check(math.abs(straight) < 0.05, 'flies level on a straight course')
    check(min_right < -0.1, 'banks right wing down into a right turn')
    check(math.abs(settled or 1) < 0.05, 'levels out on its new course')
    check(max_lean > 0.001, 'the gunship leans into a sideways move')
    ScenarioResult.done = true
end)
