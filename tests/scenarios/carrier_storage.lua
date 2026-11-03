-- Scenario test (tests/scenarios.rs): a carrier putting what it built into
-- storage (defaultcomponents.lua OnStopBuildWithStorage):
-- TransportHasAvailableStorage, then AddUnitToStorage.
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
    WaitTicks(2)
    local ok, has = pcall(carrier.TransportHasAvailableStorage, carrier)
    check(ok and has == true, 'an empty carrier has storage: ' .. tostring(has))
    if not ok then WARN('carrier storage scenario: ' .. tostring(has)) end
    -- What BuildingState does with a finished plane, then OnStopBuild.
    local plane = CreateUnitHPR('uaa0102', 'ARMY_1', wx, GetSurfaceHeight(wx, wz) + 5, wz, 0, 0, 0)
    local added = 0
    plane.OnAddToStorage = function(self, c) added = added + 1 end
    plane:AttachTo(carrier, carrier.BuildAttachBone or 0)
    local okStop, err = pcall(import('/lua/defaultcomponents.lua').ExternalFactoryComponent.OnStopBuildWithStorage, carrier, plane)
    check(okStop, 'OnStopBuildWithStorage runs: ' .. tostring(err))
    if not okStop then WARN('carrier storage scenario: ' .. tostring(err)) end
    check(added == 1, 'the plane went into storage: ' .. added)
    check(table.getn(carrier:GetCargo()) == 1, 'cargo 1: ' .. table.getn(carrier:GetCargo()))
    -- Fill the rest: then no storage is left.
    local slots = carrier.Blueprint.Transport.StorageSlots
    for i = 2, slots do
        carrier:AddUnitToStorage(CreateUnitHPR('uaa0102', 'ARMY_1', wx, GetSurfaceHeight(wx, wz) + 5, wz, 0, 0, 0))
    end
    check(table.getn(carrier:GetCargo()) == slots, 'full: ' .. table.getn(carrier:GetCargo()))
    check(carrier:TransportHasAvailableStorage() == false, 'a full carrier has no storage')
    ScenarioResult.done = true
end)
