-- Scenario test (tests/scenarios.rs): carrier storage. Three interceptors
-- ordered onto an Aeon carrier (StorageSlots) go inside (OnAddToStorage,
-- attached, cargo), not onto its attach points; an unload order lets them
-- out (OnRemoveFromStorage).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local wx, wz
    for x = 32, 992, 16 do
        for z = 32, 992, 16 do
            if GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) > 8 and GetSurfaceHeight(x + 30, z) - GetTerrainHeight(x + 30, z) > 8 then wx, wz = x, z break end
        end
        if wx then break end
    end
    if not wx then
        table.insert(ScenarioResult.notes, 'no open water on this map: skipped')
        ScenarioResult.done = true
        return
    end
    local carrier = CreateUnitHPR('uas0303', 'ARMY_1', wx, GetSurfaceHeight(wx, wz), wz, 0, 0, 0)
    local planes, log = {}, {}
    for i = 1, 3 do
        local p = CreateUnitHPR('uaa0102', 'ARMY_1', wx + 8 + i * 3, GetSurfaceHeight(wx, wz) + 15, wz + 6, 0, 0, 0)
        p.OnAddToStorage = function(self, c) table.insert(log, 'add') if c ~= carrier then table.insert(log, 'wrong carrier') end end
        p.OnRemoveFromStorage = function(self, c) table.insert(log, 'remove') end
        table.insert(planes, p)
    end
    WaitTicks(5)
    IssueTransportLoad(planes, carrier)
    for _ = 1, 300 do
        WaitTicks(1)
        if table.getn(carrier:GetCargo()) == 3 then break end
    end
    check(table.getn(carrier:GetCargo()) == 3, 'all three stored, cargo ' .. table.getn(carrier:GetCargo()))
    check(table.concat(log, ' ') == 'add add add', 'OnAddToStorage each: ' .. table.concat(log, ' '))
    check(planes[1]:IsUnitState('Attached'), 'stored units ride attached')
    IssueTransportUnload({carrier}, carrier:GetPosition())
    for _ = 1, 100 do
        WaitTicks(1)
        if table.getn(carrier:GetCargo()) == 0 then break end
    end
    check(table.getn(carrier:GetCargo()) == 0, 'unloaded')
    check(table.getn(log) == 6 and log[6] == 'remove', 'OnRemoveFromStorage each: ' .. table.concat(log, ' '))
    check(not planes[1]:IsUnitState('Attached'), 'free again')
    ScenarioResult.done = true
end)
