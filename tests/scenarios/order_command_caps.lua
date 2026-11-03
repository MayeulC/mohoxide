-- Scenario test (tests/scenarios.rs): Moho's `Issue*` bindings
-- leave out units without the order's command cap (MohoEngine.dll
-- 0x102c9780, e.g. `IssueAttack` with `RULEUCC_Attack`); move, guard and
-- patrol also need a unit that moves. A tactical launcher given
-- `IssueAttack` queues nothing, as in the real game
-- (dev/probes/maps/probe_auditgaps, lane g6); `IssueTactical` still queues.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function queue(u) return table.getn(u:GetCommandQueue() or {}) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local x, z = 200, 200
        local tml = CreateUnitHPR('ueb2108', 'ARMY_1', x + 0.5, GetTerrainHeight(x, z), z + 0.5, 0, 0, 0)
        local tank = CreateUnitHPR('uel0201', 'ARMY_1', x + 10, GetTerrainHeight(x + 10, z), z, 0, 0, 0)
        local target = CreateUnitHPR('uel0201', 'ARMY_2', x, GetTerrainHeight(x, z + 30), z + 30, 0, 0, 0)
        tml:SetFireState(1) tank:SetFireState(1) target:SetFireState(1)
        IssueAttack({ tml, tank }, target)
        check(queue(tml) == 0, 'IssueAttack leaves out the launcher (no RULEUCC_Attack): queue ' .. queue(tml))
        check(queue(tank) == 1, 'the tank takes the attack: queue ' .. queue(tank))
        IssueClearCommands({ tank })
        IssueMove({ tml }, { x + 20, 0, z })
        check(queue(tml) == 0, 'a structure takes no move: queue ' .. queue(tml))
        tml:GiveTacticalSiloAmmo(1)
        IssueTactical({ tml }, target)
        check(queue(tml) == 1, 'IssueTactical queues on the launcher: queue ' .. queue(tml))
        IssueClearCommands({ tml })
        tank:RemoveCommandCap('RULEUCC_Attack')
        IssueAttack({ tank }, target)
        check(queue(tank) == 0, 'a removed cap counts: queue ' .. queue(tank))
        tank:RestoreCommandCaps()
        IssueAttack({ tank }, target)
        check(queue(tank) == 1, 'restored caps take the order again: queue ' .. queue(tank))
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
