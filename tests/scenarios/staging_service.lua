-- Scenario test (tests/scenarios.rs): an air staging platform's service
-- (UEB5202: RefuelingMultiplier 50, RefuelingRepairAmount 500,
-- RepairConsumeEnergy 30). A damaged interceptor with 0.3 fuel docks: it
-- refuels at FuelRechargeRate 5 / FuelUseTime 300 / 10 x 50 = 0.083 a tick
-- and is repaired 50 health a tick, drawing energy.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("ENERGY", 500000) b:GiveResource("ENERGY", 500000)
    local platform = CreateUnitHPR('ueb5202', 'ARMY_1', 400, GetTerrainHeight(400, 570), 570, 0, 0, 0)
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 402, GetTerrainHeight(402, 570) + 10, 570, 0, 0, 0)
    plane:SetFuelRatio(0.3)
    plane:SetHealth(plane, 20)
    WaitTicks(2)
    IssueTransportLoad({plane}, platform)
    for _ = 1, 200 do
        WaitTicks(1)
        if plane:IsUnitState('Attached') then break end
    end
    check(plane:IsUnitState('Attached'), 'docked')
    local f0, h0, e0, t0 = plane:GetFuelRatio(), plane:GetHealth(), b:GetEconomyStored('ENERGY'), GetGameTick()
    WaitTicks(4)
    local f1, h1, e1, ticks = plane:GetFuelRatio(), plane:GetHealth(), b:GetEconomyStored('ENERGY'), GetGameTick() - t0
    note(string.format('fuel %.3f -> %.3f, health %.0f -> %.0f of %.0f, energy -%.0f', f0, f1, h0, h1, plane:GetMaxHealth(), e0 - e1))
    check(math.abs((f1 - f0) / ticks - 5 / 3000 * 50) < 0.002 or f1 == 1, 'refuels at x50')
    check(h1 > h0, 'repaired while docked')
    check(e0 - e1 > 0, 'paid for in energy')
    ScenarioResult.done = true
end)
