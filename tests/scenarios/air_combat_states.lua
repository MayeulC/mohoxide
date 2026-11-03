-- Scenario test (tests/scenarios.rs): winged aircraft fly moho's combat
-- states. A UEF T1 bomber ordered onto a tank lines up (MakingAttackRun),
-- breaks off or manoeuvres (not MakingAttackRun) and comes round again.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local bomber = CreateUnitHPR('uea0103', 'ARMY_1', 400, GetTerrainHeight(400, 520) + 20, 520, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_2', 400, GetTerrainHeight(400, 575), 575, 0, 0, 0)
    tank:GetWeapon(1):SetEnabled(false)
    tank:SetMaxHealth(100000) tank:SetHealth(tank, 100000)
    IssueAttack({bomber}, tank)
    local runs, out, changes, last = 0, 0, 0, nil
    for _ = 1, 400 do
        WaitTicks(1)
        local run = bomber:IsUnitState('MakingAttackRun')
        if run then runs = runs + 1 else out = out + 1 end
        if last ~= nil and run ~= last then changes = changes + 1 end
        last = run
    end
    note(string.format('%d ticks in a run, %d out, %d changes', runs, out, changes))
    check(runs > 0, 'lines up for attack runs (MakingAttackRun)')
    check(changes >= 3, 'leaves and re-enters its run')
    ScenarioResult.done = true
end)
