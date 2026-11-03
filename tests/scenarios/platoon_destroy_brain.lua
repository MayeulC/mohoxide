-- Scenario test (tests/scenarios.rs): a platoon's destroy callbacks get its
-- brain. FAF's `DoDestroyCallbacks` (and LOUD's `OnDestroy`) pass
-- `self:GetBrain()` to each callback, so the platoon must still exist while
-- `OnDestroy` runs: when disbanded, and when its last unit dies.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    local function platoon_with_engineer(x)
        local p = brain:MakePlatoon('probe', 'none')
        local u = CreateUnitHPR('uel0105', 'ARMY_1', x, GetTerrainHeight(x, 600), 600, 0, 0, 0)
        brain:AssignUnitsToPlatoon(p, {u}, 'Support', 'none')
        return p, u
    end
    local got = {}
    local disbanded, _ = platoon_with_engineer(400)
    disbanded:AddDestroyCallback(function(b, p) got.disband = b end)
    brain:DisbandPlatoon(disbanded)
    WaitTicks(2)
    check(got.disband == brain, 'a disbanded platoon\'s callback gets its brain')
    local emptied, unit = platoon_with_engineer(410)
    emptied:AddDestroyCallback(function(b, p) got.death = b end)
    unit:Destroy()
    WaitTicks(3)
    check(got.death == brain, 'a platoon whose last unit died passes its brain too')
    check(not brain:PlatoonExists(emptied), 'and is gone afterwards')
    ScenarioResult.done = true
end)
