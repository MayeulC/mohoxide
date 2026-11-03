-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- Reclaiming a unit takes its health down with its build fraction, as
-- moho's reclaim task does (MohoEngine.dll 0x1020a06b: SetHealth to max
-- health x min(health share, fraction) each tick), and a reclaim of a
-- damaged unit starts from its health share (0x10209e95). A reclaim broken
-- off partway leaves the unit standing, half "unbuilt" and as weak as it
-- looks (an AI's engineer re-targeting every 5 s left enemy
-- engineers at 2% built and full health: see-through and hard to kill).
ForkThread(function()
    WaitTicks(2)
    ArmyBrains[1]:__engine_SetStorage('MASS', 10000)
    ArmyBrains[1]:__engine_SetStorage('ENERGY', 10000)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 404, ground(404, 600), 600, 0, 0, 0)
    local target = CreateUnitHPR('ual0105', 'ARMY_2', 408, ground(408, 600), 600, 0, 0, 0)
    local max = target:GetMaxHealth()
    IssueReclaim({eng}, target)
    for _ = 1, 200 do
        WaitTicks(2)
        if target:GetFractionComplete() < 0.6 then break end
    end
    IssueClearCommands({eng})
    WaitTicks(3)
    local fraction, health = target:GetFractionComplete(), target:GetHealth()
    note(string.format('broken off at fraction %.3f, health %.1f / %.0f', fraction, health, max))
    check(fraction < 0.6 and fraction > 0, 'the reclaim was broken off partway, fraction ' .. fraction)
    check(not target:BeenDestroyed() and not target.Dead, 'a partly reclaimed unit stays')
    check(math.abs(health - max * fraction) <= max * 0.01, string.format('its health went down with its fraction: %.1f, want %.1f', health, max * fraction))
    -- Nobody works on it: it stays as it was (no decay: it isn't being built).
    WaitTicks(30)
    check(math.abs(target:GetFractionComplete() - fraction) < 1e-6, 'the partly reclaimed unit kept its fraction')
    check(math.abs(target:GetHealth() - health) < 1e-6, 'the partly reclaimed unit kept its health')

    -- A damaged unit: its reclaim starts from its health share, so the
    -- first tick takes its fraction down to it.
    local hurt = CreateUnitHPR('ual0105', 'ARMY_2', 404, ground(404, 604), 604, 0, 0, 0)
    local hurtMax = hurt:GetMaxHealth()
    hurt:SetHealth(nil, hurtMax * 0.3)
    IssueReclaim({eng}, hurt)
    for _ = 1, 100 do
        WaitTicks(2)
        if hurt:GetFractionComplete() < 1 then break end
    end
    local f = hurt:GetFractionComplete()
    note(string.format('damaged unit: fraction %.3f, health %.1f / %.0f', f, hurt:GetHealth(), hurtMax))
    check(f <= 0.3 + 1e-6, 'a damaged unit is reclaimed from its health share, fraction ' .. f)
    check(hurt:GetHealth() <= hurtMax * f + hurtMax * 0.01, 'and its health stays within its fraction')
    for _ = 1, 200 do
        WaitTicks(2)
        if hurt:BeenDestroyed() then break end
    end
    check(hurt:BeenDestroyed(), 'the damaged unit was reclaimed to nothing')
    done()
end)
