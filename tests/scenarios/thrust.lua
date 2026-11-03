-- Scenario test (tests/scenarios.rs): UEF gunship engine pods
-- (CreateThrustController on Jet_Front / Jet_Back, UEA0203_script.lua:26)
-- tilt with the unit's motion: one way at rest, another while it speeds up.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function dir(u, bone) local x, y, z = u:GetBoneDirection(bone) return { x, y, z } end
local function angle(a, b) return math.deg(math.acos(math.min(1, a[1] * b[1] + a[2] * b[2] + a[3] * b[3]))) end
ForkThread(function()
    WaitTicks(2)
    local ship = CreateUnitHPR('uea0203', 'ARMY_1', 400, GetTerrainHeight(400, 600) + 20, 600, 0, 0, 0)
    WaitTicks(40)
    local rest = dir(ship, 'Jet_Front')
    IssueMove({ ship }, { 400, GetTerrainHeight(400, 750), 750 })
    local most = 0
    for _ = 1, 20 do
        WaitTicks(1)
        most = math.max(most, angle(rest, dir(ship, 'Jet_Front')))
    end
    note(string.format('pod tilt while speeding up: %.1f degrees', most))
    check(most > 5, string.format('the pod tilts as it speeds up: %.1f', most))
    ScenarioResult.done = true
end)
