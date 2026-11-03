-- Scenario test (tests/scenarios.rs): point defenses hit what they shoot
-- at. The pitch is solved from the muzzle (where the shot leaves), not the
-- turret's pivot: aimed from the pivot, shots passed a barrel's height over
-- the target, clean over small units (the Ravager, Cybran and Aeon T2 PDs
-- did no damage at all to a Mech Marine).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        -- Some PDs charge each shot from energy (xeb2306: EnergyRequired).
        GetArmyBrain('ARMY_1'):__engine_SetStorage('ENERGY', 100000)
        GetArmyBrain('ARMY_1'):GiveResource('ENERGY', 100000)
        for _, id in { 'ueb2101', 'ueb2301', 'xeb2306', 'urb2101', 'urb2301', 'uab2101', 'uab2301', 'xsb2101', 'xsb2301' } do
            for _, target in { 'uel0106', 'uel0201' } do
                for _, d in { 8, 20 } do
                    local x, z = 400, 600
                    local pd = CreateUnitHPR(id, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
                    local tx, tz = x + d * 0.6, z + d * 0.8
                    local t = CreateUnitHPR(target, 'ARMY_2', tx, GetTerrainHeight(tx, tz), tz, 0, 0, 0)
                    t:SetMaxHealth(100000) t:SetHealth(t, 100000)
                    WaitTicks(100)
                    local dealt = 100000 - t:GetHealth()
                    note(string.format('%s vs %s %d away: %.0f damage', id, target, d, dealt))
                    check(dealt > 0, string.format('%s hits a %s %d away', id, target, d))
                    t:Destroy() pd:Destroy()
                    WaitTicks(2)
                end
            end
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
