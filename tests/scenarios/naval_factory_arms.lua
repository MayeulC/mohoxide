-- Scenario test (tests/scenarios.rs): sliders move a bone along its own
-- axes, not its parent's. The UEF naval factories' arm bones are modelled
-- turned (local +x runs along the factory, mesh +z), and their scripts
-- (UEB0103_script.lua) slide them with x goals: the arm must run along the
-- factory's length, not out over its side.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local f = CreateUnitHPR('ueb0103', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local p0 = f:GetPosition('Right_Arm')
    local slider = CreateSlider(f, 'Right_Arm', 40, 0, 0, -1)
    WaitTicks(2)
    local p = f:GetPosition('Right_Arm')
    local dx, dy, dz = p[1] - p0[1], p[2] - p0[2], p[3] - p0[3]
    table.insert(ScenarioResult.notes, string.format('Right_Arm moved %.2f %.2f %.2f', dx, dy, dz))
    check(math.abs(dz) > 1, string.format('the arm slides along the factory (+z): %.2f %.2f %.2f', dx, dy, dz))
    check(math.abs(dx) < 0.05 and math.abs(dy) < 0.05, string.format('not sideways or up: %.2f %.2f %.2f', dx, dy, dz))
    slider:Destroy()
    ScenarioResult.done = true
end)
