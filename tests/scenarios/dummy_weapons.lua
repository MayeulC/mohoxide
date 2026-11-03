-- Scenario test (tests/scenarios.rs): blueprint weapons marked `DummyWeapon`
-- get no engine weapon (moho's description: "the engine should not create
-- an actual weapon for this blueprint"): the UEF ACU's TeleportWeapon and
-- DeathWeapon are data FAF's scripts read, so its weapon count is its real
-- weapons only and the labels find nothing.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local real, dummies = 0, {}
    for _, w in acu:GetBlueprint().Weapon do
        if w.DummyWeapon then table.insert(dummies, w.Label) else real = real + 1 end
    end
    check(table.getn(dummies) > 0, 'the ACU has dummy weapons')
    check(acu:GetWeaponCount() == real, 'weapon count is the real weapons: ' .. acu:GetWeaponCount() .. ' vs ' .. real)
    for _, label in dummies do
        check(acu:GetWeaponByLabel(label) == nil, 'no weapon for dummy ' .. label)
    end
    ScenarioResult.done = true
end)
