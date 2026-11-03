-- Scenario test (tests/scenarios.rs): a UEF T1 land factory upgrading to T2
-- plays FAF's upgrade animation (`AnimationUpgradeTable`), its fraction
-- following the T2's build. FAF's clip has every frame stamped time 0; its
-- frames are spread over the clip's 1.5 s (`assets::sca`), so it opens up
-- gradually instead of jumping to its last frame.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local b = ArmyBrains[1]
        b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
        b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
        local f = CreateUnitHPR('ueb0101', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
        WaitTicks(5)
        IssueUpgrade({ f }, 'ueb0201')
        WaitTicks(20)
        local anim = f.AnimatorUpgradeManip
        check(f:IsUnitState('Upgrading'), 'the factory is upgrading')
        check(anim ~= nil, 'with its upgrade animation')
        if not anim then return end
        local first = anim:GetAnimationTime()
        local lift_first = f:GetPosition('Lifter_Left')[2]
        WaitTicks(80)
        local later = anim:GetAnimationTime()
        local lift_later = f:GetPosition('Lifter_Left')[2]
        note(string.format('Lifter_Left %.2f -> %.2f high', lift_first, lift_later))
        check(lift_later > lift_first + 0.05, 'the lifter rises as the upgrade goes on (not already at the top)')
        note(string.format('upgrade clip %.2f s: at %.3f s, then %.3f s; T2 built %.3f', anim:GetAnimationDuration(), first, later, f.UnitBeingBuilt:GetFractionComplete()))
        check(later > first and later < anim:GetAnimationDuration(), 'the animation moves with the build, not straight to its end')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
