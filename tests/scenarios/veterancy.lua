-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
--
-- Veterancy: killing an enemy accrues mass credit, levels the killer up
-- (max-health buff, regen buff, heal), and the stats are readable back
-- through `GetStat` (what the interface's `UserUnit:GetStat` returns).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end

ForkThread(function()
    WaitTicks(2)
    -- On flat land. A Striker on the seabed (256, 256 is under water) has
    -- no FireTargetLayerCapsTable entry for its layer, so FAF sets its caps
    -- to 'None' and it never fires; over a bump its low-arc shells hit the
    -- ground (the line from 412 to 428 at z 600 has a 0.6 ridge).
    local cx, cz = 436, 570
    local worst = 0
    for x = cx - 8, cx + 8, 0.5 do
        local line = GetTerrainHeight(cx - 8, cz) + (GetTerrainHeight(cx + 8, cz) - GetTerrainHeight(cx - 8, cz)) * (x - cx + 8) / 16
        worst = math.max(worst, math.abs(GetTerrainHeight(x, cz) - line))
    end
    check(worst < 0.05, 'ground between the units is flat, off by ' .. worst)
    local champ = CreateUnitHPR('uel0201', 'ARMY_1', cx - 8, GetTerrainHeight(cx - 8, cz), cz, 0, math.pi / 2, 0)
    check(champ.Blueprint.VetEnabled == true, 'striker blueprint is VetEnabled')
    check(champ.VetLevel == 0 and champ.VetExperience == 0, 'vet starts at 0/0')
    check(champ:GetStat('VetLevel', -1).Value == 0, 'VetLevel stat starts at 0')
    check(champ:GetCurrentLayer() == 'Land', 'striker stands on land, got ' .. tostring(champ:GetCurrentLayer()))
    local maxhp = champ:GetMaxHealth()
    note('thresholds: ' .. table.concat(champ.Blueprint.VetThresholds, ','))

    -- Two mantis kills (56 mass each) reach the striker's first threshold (112).
    for i = 1, 2 do
        local victim = CreateUnitHPR('url0107', 'ARMY_2', cx + 8, GetTerrainHeight(cx + 8, cz), cz, 0, -math.pi / 2, 0)
        victim:SetMaxHealth(5)
        victim:SetHealth(victim, 5)
        IssueAttack({ champ }, victim)
        for _ = 1, 200 do
            WaitTicks(2)
            if victim.Dead then break end
        end
        check(victim.Dead, 'victim ' .. i .. ' died')
        check(champ.VetMassKillCredit == 56 * i, 'credit is 56 x ' .. i .. ', got ' .. tostring(champ.VetMassKillCredit))
    end
    check(champ.VetExperience == 112, 'experience is 112, got ' .. tostring(champ.VetExperience))
    check(champ.VetLevel == 1, 'leveled to 1, got ' .. tostring(champ.VetLevel))
    check(champ:GetStat('VetLevel', -1).Value == 1, 'VetLevel stat reads back 1')
    check(champ:GetStat('VetExperience', -1).Value == 112, 'VetExperience stat reads back 112')
    check(champ:GetStat('VetMassKillCredit', -1).Value == 112, 'VetMassKillCredit stat reads back 112')
    check(champ:GetMaxHealth() > maxhp, 'max health rose with the level (' .. maxhp .. ' -> ' .. champ:GetMaxHealth() .. ')')
    check(champ.Buffs.BuffTable['VeterancyRegen'] ~= nil, 'a VeterancyRegen buff is applied')

    -- The regen buff heals over time (once the mantis's last shells, still
    -- in flight when it died, have landed).
    WaitTicks(20)
    champ:AdjustHealth(champ, -50)
    local hurt = champ:GetHealth()
    for _ = 1, 50 do WaitTicks(2) end
    check(champ:GetHealth() > hurt, 'regen healed ' .. hurt .. ' -> ' .. champ:GetHealth())
    done()
end)
