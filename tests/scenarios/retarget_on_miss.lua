-- Scenario test (tests/scenarios.rs): `ReTargetOnMiss`. A UEF T1 point
-- defence (the flag set) with two enemy tanks in range: the nearer one
-- has no collision volume, so every shot at it flies through and lands
-- behind. Missing a still target 11 times in a row, the weapon gives it up
-- and takes the other tank. Without the flag it would shoot at the first
-- forever.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end

ForkThread(function()
    WaitTicks(2)
    local ok, e = pcall(function()
        local x, z = 400, 600
        local pd = CreateUnitHPR('ueb2101', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
        local ghost = CreateUnitHPR('uel0201', 'ARMY_2', x + 10, GetTerrainHeight(x + 10, z), z, 0, 0, 0)
        local real = CreateUnitHPR('uel0201', 'ARMY_2', x - 18, GetTerrainHeight(x - 18, z), z, 0, 0, 0)
        for _, t in { ghost, real } do
            t:SetFireState(1)
            t:SetMaxHealth(1e7)
            t:SetHealth(nil, 1e7)
        end
        ghost:SetCollisionShape('None')
        local shots = 0
        local w = pd:GetWeapon(1)
        local orig = w.CreateProjectileForWeapon
        w.CreateProjectileForWeapon = function(self, ...)
            shots = shots + 1
            return orig(self, ...)
        end
        local switched
        for i = 1, 600 do
            WaitTicks(1)
            if not switched and real:GetHealth() < 1e7 then switched = shots end
        end
        note(string.format('%d shots; the other tank first damaged after shot %s', shots, tostring(switched)))
        check(switched ~= nil, 'the point defence gives up the tank it keeps missing and shoots the other')
        check(switched == nil or switched >= 11, 'not before 11 misses')
    end)
    if not ok then check(false, tostring(e)) end
    ScenarioResult.done = true
end)
