-- Scenario test (tests/scenarios.rs): bombers lead moving targets
-- (PredictAheadForBombDrop) and release toward them (RealisticOrdinance
-- bombs are aimed horizontally at the target, or where it will be). A UEF
-- T1 bomber (uea0103: 3 s ahead) attacks Strikers driving across its path.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local bomber = CreateUnitHPR('uea0103', 'ARMY_1', 400, GetTerrainHeight(400, 520) + 20, 520, 0, 0, 0)
    local tanks, hp0 = {}, 0
    for i = 1, 3 do
        local t = CreateUnitHPR('uel0201', 'ARMY_2', 350 + i * 4, GetTerrainHeight(350, 580), 575 + i * 2, 0, 0, 0)
        for _, w in ipairs({t:GetWeapon(1)}) do w:SetEnabled(false) end
        IssueMove({t}, {460, GetTerrainHeight(460, 580), 575 + i * 2})
        hp0 = hp0 + t:GetHealth()
        tanks[i] = t
    end
    IssueAttack({bomber}, tanks[2])
    WaitTicks(150)
    local hp = 0
    for _, t in ipairs(tanks) do if not t:BeenDestroyed() then hp = hp + t:GetHealth() end end
    note(string.format('damage dealt %.0f of %.0f', hp0 - hp, hp0))
    check(hp0 - hp >= 120, 'the bombs hit the moving tanks (without leading: 47)')
    ScenarioResult.done = true
end)
