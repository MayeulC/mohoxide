-- Scenario test (tests/scenarios.rs): OverchargeWeapon.CanOvercharge and
-- PauseOvercharge read unit:IsOverchargePaused(), which follows
-- SetOverchargePaused.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    check(acu:IsOverchargePaused() == false, 'a new unit is not overcharge paused')
    local oc = acu:GetWeaponByLabel('OverCharge')
    local ok, err = pcall(oc.CanOvercharge, oc)
    check(ok, 'CanOvercharge runs: ' .. tostring(err))
    acu:SetOverchargePaused(true)
    check(acu:IsOverchargePaused() == true, 'SetOverchargePaused(true) is read back')
    check(oc:CanOvercharge() == false, 'a paused overcharge cannot fire')
    acu:SetOverchargePaused(false)
    check(acu:IsOverchargePaused() == false, 'SetOverchargePaused(false) is read back')
    -- PauseOvercharge holds the pause for 1 / RateOfFire seconds, then clears it.
    local thread = ForkThread(oc.PauseOvercharge, oc)
    WaitTicks(2)
    check(acu:IsOverchargePaused() == true, 'PauseOvercharge pauses')
    WaitSeconds(1 / oc.Blueprint.RateOfFire + 0.5)
    check(acu:IsOverchargePaused() == false, 'PauseOvercharge unpauses after its cool-down')
    ScenarioResult.done = true
end)
