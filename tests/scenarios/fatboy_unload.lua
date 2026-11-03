-- Scenario test (tests/scenarios.rs): a Fatboy ordered to unload lets what
-- it carries go without touching its plain `AttachTo` children:
-- the external factory's anchor entity and `ZXA0003` dummy have no
-- `OnDetachedFromTransport`, so walking the whole attachment chain errors in
-- `OnTransportDetach` (/lua/sim/unit.lua).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local cx, cz = 420, 600
        local fatboy = CreateUnitHPR('uel0401', 'ARMY_1', cx, GetTerrainHeight(cx, cz), cz, 0, 0, 0)
        WaitTicks(20)
        check(fatboy.ExternalFactory ~= nil, 'the Fatboy has its external factory')

        local plane = CreateUnitHPR('uea0102', 'ARMY_1', cx + 30, GetTerrainHeight(cx + 30, cz) + 20, cz, 0, 0, 0)
        IssueTransportLoad({plane}, fatboy)
        for _ = 1, 300 do
            WaitTicks(1)
            if plane:IsUnitState('Attached') then break end
        end
        check(plane:IsUnitState('Attached'), 'the plane docked')
        if not plane:IsUnitState('Attached') then return end

        local detached, errors = 0, 0
        local orig = fatboy.OnTransportDetach
        fatboy.OnTransportDetach = function(self, bone, unit)
            detached = detached + 1
            local ok2, e = pcall(orig, self, bone, unit)
            if not ok2 then errors = errors + 1 note('detach error: ' .. tostring(e)) end
        end

        local px, py, pz = fatboy:GetPositionXYZ()
        IssueTransportUnload({fatboy}, {px, py, pz})
        for _ = 1, 100 do
            WaitTicks(1)
            if not plane:IsUnitState('Attached') then break end
        end
        check(not plane:IsUnitState('Attached'), 'the plane was let go')
        check(errors == 0, 'no detach errors (' .. errors .. ')')
        check(detached == 1, 'OnTransportDetach once, for the plane only: ' .. detached)
        check(not fatboy.ExternalFactory.Dead, 'the external factory survived the unload')
        WaitTicks(10)
        check(not plane.Dead, 'the plane lives on')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
