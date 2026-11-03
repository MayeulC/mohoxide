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
-- Fuel drains in flight; at empty FAF's OnRunOutOfFuel applies its penalty.
ForkThread(function()
    WaitTicks(2)
    local p = CreateUnitHPR('uea0102', 'ARMY_1', 380, 40, 600, 0, 0, 0)
    check(p:GetFuelRatio() == 1, 'starts full')
    p:SetFuelUseTime(5)
    IssueMove({p}, {800, 0, 600})
    WaitTicks(25)
    local r = p:GetFuelRatio()
    check(r > 0.45 and r < 0.55, 'half a 5 s tank after 2.5 s, got ' .. r)
    WaitTicks(40)
    check(p:GetFuelRatio() == 0, 'empty')
    check(p.HasFuel == false, 'OnRunOutOfFuel ran')
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, ground(400, 620), 620, 0, 0, 0)
    check(tank:GetFuelRatio() == -1, 'units without fuel report -1')
    done()
end)
