-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- Footprint classes: a wall of structures runs north-south across the
-- tank's and the Fatboy's way east. The tank (Vehicle2x2) has to go round
-- its end; the Fatboy (Amphibious6x6, IgnoreStructures) walks straight over.
ForkThread(function()
    WaitTicks(2)
    for z = 560, 640 do
        for x = 410, 411 do
            CreateUnitHPR('ueb5101', 'ARMY_2', x + 0.5, ground(x + 0.5, z + 0.5), z + 0.5, 0, 0, 0)
        end
    end
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 395, ground(395, 590), 590, 0, 0, 0)
    local fatboy = CreateUnitHPR('uel0401', 'ARMY_1', 392, ground(392, 612), 612, 0, 0, 0)
    IssueStop({tank, fatboy})
    local tankCmd = IssueMove({tank}, {425, 0, 590})
    local fatCmd = IssueMove({fatboy}, {428, 0, 612})
    local tankOff, fatOff = 0, 0
    for _ = 1, 70 do
        WaitTicks(10)
        tankOff = math.max(tankOff, math.abs(tank:GetPosition()[3] - 590))
        fatOff = math.max(fatOff, math.abs(fatboy:GetPosition()[3] - 612))
        if IsCommandDone(tankCmd) and IsCommandDone(fatCmd) then break end
    end
    check(IsCommandDone(fatCmd), 'the Fatboy crossed the wall')
    check(fatOff < 4, 'the Fatboy went straight over, strayed ' .. fatOff)
    check(IsCommandDone(tankCmd), 'the tank got round the wall')
    check(tankOff > 25, 'the tank went round the end, strayed ' .. tankOff)
    note('tank strayed ' .. tankOff .. ', fatboy ' .. fatOff)
    done()
end)
