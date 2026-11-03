-- Scenario test (tests/scenarios.rs): SetMaxHealth only sets the maximum
-- and leaves health alone, even above it (moho's Entity::SetMaxHealth
-- stores the value and nothing else, MohoEngine.dll 0x10167af0 and the
-- binding at 0x10270d50); SetHealth / AdjustHealth clamp (the SetHealth
-- binding adjusts by the difference, and AdjustHealth, 0x1025e1a0, clamps
-- to [0, max]). FAF's veterancy swaps its DoNotFill max-health buff down to
-- the base and up again before its heal: health above the base is kept.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local Buff = import('/lua/sim/buff.lua')
ForkThread(function()
    WaitTicks(2)
    local u = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    u:SetRegenRate(0)
    local base = u:GetMaxHealth()
    check(u:GetHealth() == base, 'starts full: ' .. u:GetHealth() .. ' / ' .. base)

    -- Plain calls.
    u:SetMaxHealth(base / 2)
    check(u:GetMaxHealth() == base / 2, 'max set to ' .. base / 2 .. ', got ' .. u:GetMaxHealth())
    check(u:GetHealth() == base, 'SetMaxHealth below health leaves health at ' .. base .. ', got ' .. u:GetHealth())
    u:SetMaxHealth(base)
    check(u:GetHealth() == base, 'and raising it back leaves it too, got ' .. u:GetHealth())
    u:SetHealth(u, base * 2)
    check(u:GetHealth() == base, 'SetHealth clamps to max, got ' .. u:GetHealth())
    u:AdjustHealth(u, base)
    check(u:GetHealth() == base, 'AdjustHealth clamps to max, got ' .. u:GetHealth())

    -- Veterancy's buff swap: level 1 (x1.1, DoNotFill), healed full, then
    -- level 2 (x1.2) replaces it: the removal takes max down to the base first.
    Buff.ApplyBuff(u, 'VeterancyMaxHealth1')
    local max1 = u:GetMaxHealth()
    check(max1 > base and u:GetHealth() == base, 'level 1 raises max only: ' .. u:GetHealth() .. ' / ' .. max1)
    u:AdjustHealth(u, max1)
    check(u:GetHealth() == max1, 'healed to ' .. max1 .. ', got ' .. u:GetHealth())
    Buff.ApplyBuff(u, 'VeterancyMaxHealth2')
    local max2 = u:GetMaxHealth()
    check(max2 > max1, 'level 2 raises max again: ' .. max1 .. ' -> ' .. max2)
    check(u:GetHealth() == max1, 'level 2 keeps the health from level 1 (' .. max1 .. '), got ' .. u:GetHealth())

    -- A max-health enhancement buff (as the ACUs' NewHealth ones) without
    -- DoNotFill heals by the max it adds, not to full.
    u:SetHealth(u, max2 / 2)
    local before = u:GetHealth()
    BuffBlueprint { Name = 'MaxHealthScenarioAdd', DisplayName = 'MaxHealthScenarioAdd', BuffType = 'MAXHEALTHSCENARIO',
        Stacks = 'ALWAYS', Duration = -1, Affects = { MaxHealth = { Add = 100, Mult = 1.0 } } }
    Buff.ApplyBuff(u, 'MaxHealthScenarioAdd')
    local added = u:GetMaxHealth() - max2
    check(added > 0, 'the add raised max, by ' .. added)
    check(math.abs(u:GetHealth() - (before + added)) < 0.01, 'and health by the same ' .. added .. ': ' .. before .. ' -> ' .. u:GetHealth())
    ScenarioResult.done = true
end)
