-- AI-vs-AI census (manual long run): every 5 game minutes, logs each AI's
-- unit counts and economy (CENSUS lines) and its commander's enhancements
-- (ACUENH lines). Run it in release with a log file, then read the counts and
-- the Lua error tally (docs/guides/running.md, "Long AI games").
ForkThread(function()
    for _, t in {6000, 12000, 18000} do
        WaitTicks(t - GetGameTick())
        for _, brain in ArmyBrains do
            if brain:GetArmyIndex() > 2 then break end
            local s = ''
            for _, u in brain:GetListOfUnits(categories.COMMAND, false) or {} do
                local e = import('/lua/enhancementcommon.lua').GetEnhancements(u.EntityId) or {}
                for slot, name in sortedpairs(e) do s = s .. ' ' .. slot .. '=' .. name end
            end
            LOG(string.format('ACUENH t=%d army %d:%s', GetGameTick(), brain:GetArmyIndex(), s))
        end
    end
end)
ForkThread(function()
    for _, t in {3000, 6000, 9000, 12000} do
        WaitTicks(t - GetGameTick())
        for _, brain in ArmyBrains do
            if brain:GetArmyIndex() > 2 then break end
            local counts, total = {}, 0
            for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
                counts[u.UnitId] = (counts[u.UnitId] or 0) + 1
                total = total + 1
            end
            local s = ''
            for k, v in sortedpairs(counts) do s = s .. ' ' .. k .. 'x' .. v end
            LOG(string.format('CENSUS t=%d army %d units=%d mass=%.0f energy=%.0f:%s', GetGameTick(), brain:GetArmyIndex(), total, brain:GetEconomyStored('MASS'), brain:GetEconomyStored('ENERGY'), s))
        end
    end
end)
