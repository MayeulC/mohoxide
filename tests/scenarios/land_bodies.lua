-- Scenario test (tests/scenarios.rs): land unit bodies. A hover tank (Aeon
-- Aurora) floats at its Physics.Elevation over the ground; an idle Fatboy
-- isn't shoved aside by a column of T1 tanks driving through it.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local aurora = CreateUnitHPR('ual0201', 'ARMY_1', 400, GetTerrainHeight(400, 640), 640, 0, 0, 0)
    IssueMove({aurora}, {430, 0, 640})
    WaitTicks(20)
    local a = aurora:GetPosition()
    local over = a[2] - GetSurfaceHeight(a[1], a[3])
    note(string.format('aurora %.3f over the surface', over))
    check(math.abs(over - aurora.Blueprint.Physics.Elevation) < 0.05, string.format('the Aurora hovers at its elevation (%.3f)', over))

    -- Its up axis (orientation x, y, z, w): the angle from vertical.
    local function tilt(u)
        local o = u:GetOrientation()
        local x, y, z, w = o[1], o[2], o[3], o[4]
        local upy = 1 - 2 * (x * x + z * z)
        return math.deg(math.acos(math.max(-1, math.min(1, upy))))
    end
    WaitTicks(40)
    local lo, hi = 1e9, 0
    for _ = 1, 40 do
        WaitTicks(1)
        local t = tilt(aurora)
        lo, hi = math.min(lo, t), math.max(hi, t)
    end
    note(string.format('idle aurora tilt %.2f..%.2f deg', lo, hi))
    check(hi - lo > 0.3 and hi < 12, string.format('the idle Aurora wobbles (tilt %.2f..%.2f)', lo, hi))
    -- A tank over the hills tilts with the ground.
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    IssueMove({tank}, {440, 0, 600})
    local most = 0
    for _ = 1, 60 do
        WaitTicks(1)
        most = math.max(most, tilt(tank))
    end
    note(string.format('tank tilt up to %.1f deg', most))
    check(most > 2, string.format('a tank tilts on the slopes (%.1f deg)', most))
    tank:Destroy()

    -- A Fatboy driving east meets a column of T1 tanks driving west.
    local fatboy = CreateUnitHPR('uel0401', 'ARMY_1', 650, GetTerrainHeight(650, 320), 320, 0, math.pi / 2, 0)
    WaitTicks(5)
    local tanks = {}
    for i = 0, 23 do
        local x, z = 695 + (i % 4) * 2, 318 + math.floor(i / 4) * 1
        table.insert(tanks, CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0))
    end
    IssueMove({fatboy}, {705, 0, 320})
    IssueMove(tanks, {645, 0, 320})
    local worst = 0
    for _ = 1, 250 do
        WaitTicks(1)
        local f = fatboy:GetPosition()
        worst = math.max(worst, math.abs(f[3] - 320))
    end
    note(string.format('fatboy pushed off its line by up to %.2f', worst))
    -- (Its own path wiggles ~1.1 off the line with no tanks at all.)
    check(worst < 1.5, string.format('the Fatboy holds its line through T1 tanks (off by %.2f)', worst))
    ScenarioResult.done = true
end)
