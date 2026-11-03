-- Scenario test (tests/scenarios.rs): a Fatboy killed while its external
-- factory builds: what it was carrying is let go without a script error
-- (`OnTransportDetach` / `OnDetachedFromTransport`), nothing is left
-- attached to the wreck.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local b = ArmyBrains[1]
        b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
        b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
        local fatboy = CreateUnitHPR('uel0401', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
        WaitTicks(20)
        local factory = fatboy.ExternalFactory
        check(factory ~= nil, 'the Fatboy has its external factory')
        IssueBuildFactory({ fatboy }, 'uel0201', 3)
        local built
        for _ = 1, 200 do
            WaitTicks(1)
            built = (factory and factory.UnitBeingBuilt) or fatboy.UnitBeingBuilt
            if built and built:GetFractionComplete() > 0.3 then break end
        end
        note('factory queue ' .. tostring(factory and table.getn(factory:GetCommandQueue())) .. ', fatboy queue ' .. table.getn(fatboy:GetCommandQueue()))
        note('building: ' .. tostring(built and built.UnitId) .. ' at ' .. tostring(built and built:GetFractionComplete()))
        -- Count script errors from the detach callbacks.
        local errors = 0
        local orig = fatboy.OnTransportDetach
        fatboy.OnTransportDetach = function(self, bone, unit)
            local ok2, e = pcall(orig, self, bone, unit)
            if not ok2 then errors = errors + 1 note('detach error: ' .. tostring(e)) end
        end
        fatboy:Kill()
        WaitTicks(100)
        check(errors == 0, 'no detach errors (' .. errors .. ')')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
