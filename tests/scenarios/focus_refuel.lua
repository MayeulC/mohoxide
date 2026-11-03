-- Scenario test (tests/scenarios.rs): OnAssignedFocusEntity when a unit
-- gets a new focus (SetFocusEntity, starting a build), and OnStartRefueling
-- when an aircraft short of fuel docks at an air staging platform.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local other = CreateUnitHPR('uel0201', 'ARMY_1', 404, GetTerrainHeight(404, 600), 600, 0, 0, 0)
    local assigned = 0
    eng.OnAssignedFocusEntity = function(self) assigned = assigned + 1 end
    WaitTicks(1)
    eng:SetFocusEntity(other)
    eng:SetFocusEntity(other)
    check(assigned == 1, 'one call for one new focus: ' .. assigned)
    IssueBuildMobile({eng}, {410, GetTerrainHeight(410, 600), 600}, 'ueb1101', {})
    for _ = 1, 100 do
        WaitTicks(1)
        if eng:IsUnitState('Building') then break end
    end
    check(eng:IsUnitState('Building'), 'engineer started building')
    check(assigned == 2, 'new focus when the build starts: ' .. assigned)

    local platform = CreateUnitHPR('ueb5202', 'ARMY_1', 360, GetTerrainHeight(360, 640), 640, 0, 0, 0)
    local plane = CreateUnitHPR('uea0102', 'ARMY_1', 362, GetTerrainHeight(362, 640) + 10, 640, 0, 0, 0)
    local refuelling = false
    local orig = plane.OnStartRefueling
    plane.OnStartRefueling = function(self) refuelling = true; return orig(self) end
    -- Moho starts refuelling only below a full tank.
    plane:SetFuelRatio(0.5)
    WaitTicks(2)
    IssueTransportLoad({plane}, platform)
    for _ = 1, 100 do
        WaitTicks(1)
        if refuelling then break end
    end
    check(refuelling, 'OnStartRefueling on docking')
    check(plane:IsUnitState('Refueling'), 'in the Refueling state')
    ScenarioResult.done = true
end)
