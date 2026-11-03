-- Scenario test (tests/scenarios.rs): a Damage buff (Buff.lua:347) calls
-- weapon:ChangeDamage on each weapon of the unit.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    BuffBlueprint {
        Name = 'ScenarioDamageBuff',
        DisplayName = 'ScenarioDamageBuff',
        BuffType = 'SCENARIODAMAGE',
        Stacks = 'REPLACE',
        Duration = -1,
        Affects = { Damage = { Add = 10, Mult = 1 } },
    }
    local Buff = import('/lua/sim/Buff.lua')
    local changes = {}
    local w = tank:GetWeapon(1)
    local change = w.ChangeDamage
    w.ChangeDamage = function(self, v) table.insert(changes, v) return change(self, v) end
    local ok, err = pcall(Buff.ApplyBuff, tank, 'ScenarioDamageBuff')
    check(ok, 'ApplyBuff runs: ' .. tostring(err))
    if not ok then WARN('damage buff scenario: ' .. tostring(err)) end
    check(changes[1] == w:GetBlueprint().Damage + 10, 'ChangeDamage with blueprint damage + 10: ' .. tostring(changes[1]))
    ScenarioResult.done = true
end)
