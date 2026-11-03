-- Scenario test (tests/scenarios.rs): IsBlip tells blips from units, as the
-- attack-order check in commands/shared.lua:133 needs; MetaImpact (only a
-- dead projectile uses it) doesn't raise.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local mine = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local theirs = CreateUnitHPR('uel0201', 'ARMY_2', 405, GetTerrainHeight(405, 600), 600, 0, 0, 0)
    WaitTicks(5)
    local ok, err = pcall(function()
        local blip = theirs:GetBlip(1)
        check(blip ~= nil, 'army 1 has a blip of the enemy tank')
        check(IsBlip(blip) == true, 'a blip is a blip')
        check(IsBlip(theirs) == false, 'a unit is not')
        check(IsBlip(nil) == false and IsBlip(5) == false, 'nor other values')
        MetaImpact(mine, mine:GetPosition(), 2, 2)
    end)
    check(ok, 'runs: ' .. tostring(err))
    if not ok then WARN('is blip scenario: ' .. tostring(err)) end
    ScenarioResult.done = true
end)
