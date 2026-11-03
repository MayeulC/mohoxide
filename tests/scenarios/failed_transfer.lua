-- Scenario test (tests/scenarios.rs): ChangeUnitArmy into an army at its
-- unit cap is refused (nil, the unit stays) and the giving army's brain
-- hears OnFailedUnitTransfer; with room, the transfer goes through.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    CreateUnitHPR('uel0201', 'ARMY_2', 410, GetTerrainHeight(410, 600), 600, 0, 0, 0)
    local failed = {0, 0}
    for i = 1, 2 do
        ArmyBrains[i].OnFailedUnitTransfer = function(self) failed[i] = failed[i] + 1 end
    end
    SetArmyUnitCap(2, math.ceil(GetArmyUnitCostTotal(2)))
    local moved = ChangeUnitArmy(tank, 2)
    check(moved == nil, 'refused at the cap')
    check(not tank.Dead and tank:GetArmy() == 1, 'the unit stays with its army')
    check(failed[1] == 1 and failed[2] == 0, 'the giver hears it once: ' .. failed[1] .. ', ' .. failed[2])
    SetArmyUnitCap(2, GetArmyUnitCap(2) + 10)
    moved = ChangeUnitArmy(tank, 2)
    check(moved ~= nil and moved:GetArmy() == 2, 'goes through with room')
    check(failed[1] == 1, 'no failure with room')
    ScenarioResult.done = true
end)
