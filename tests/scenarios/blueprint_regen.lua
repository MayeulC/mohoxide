-- Scenario test (tests/scenarios.rs): a unit regenerates its blueprint's
-- Defense.RegenRate without any buff (moho seeds the regen attribute from
-- the blueprint when the unit is made, MohoEngine.dll 0x102845ed), and
-- RevertRegenRate puts it back after SetRegenRate.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local rate = acu:GetBlueprint().Defense.RegenRate
    acu:SetHealth(acu, acu:GetMaxHealth() / 2)
    local before = acu:GetHealth()
    WaitTicks(20)
    local gained = acu:GetHealth() - before
    check(math.abs(gained - rate * 2) <= rate * 0.2, 'regenerates RegenRate (' .. rate .. ') per second: gained ' .. gained .. ' in 2 s')
    acu:SetRegenRate(0)
    before = acu:GetHealth()
    WaitTicks(10)
    check(acu:GetHealth() == before, 'SetRegenRate(0) stops it')
    acu:RevertRegenRate()
    before = acu:GetHealth()
    WaitTicks(10)
    check(acu:GetHealth() > before, 'RevertRegenRate restores it')
    ScenarioResult.done = true
end)
