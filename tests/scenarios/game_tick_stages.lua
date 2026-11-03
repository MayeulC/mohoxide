-- Scenario test (tests/scenarios.rs): every stage of a sim beat reads the
-- same `GetGameTick()`. The real game's `Sim::AdvanceBeat`
-- (MohoEngine.dll 0x10318f50) adds 1 to the tick first (0x10318fed), then
-- runs its stages, so a weapon's `OnGotTarget` / `OnFire` (weapon stage)
-- and the Lua threads of that beat read the same number (dev/probes
-- probe_firsttick: `onfire` and the fire thread's first line on the same
-- tick). Ours advanced the tick in the threads stage until 2026-10-31, so
-- weapon callbacks read one lower than the threads of their beat.
-- A thread polling every beat (it runs after the weapon stage) sees each
-- callback in the beat it happened and compares the ticks.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function ground(x, z) return GetTerrainHeight(x, z) end
SetAlliance('ARMY_1', 'ARMY_2', 'Enemy')
SetAlliance('ARMY_2', 'ARMY_1', 'Enemy')
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local target = CreateUnitHPR('uel0201', 'ARMY_2', 410, ground(410, 600), 600, 0, 0, 0)
    target:SetFireState(1)
    target:SetMaxHealth(1e6)
    target:SetHealth(target, 1e6)
    local w = tank:GetWeaponByLabel('MainGun')
    local events = {}
    w.OnGotTarget = function(self)
        table.insert(events, { 'OnGotTarget', GetGameTick() })
        return getmetatable(self).OnGotTarget(self)
    end
    w.OnFire = function(self)
        table.insert(events, { 'OnFire', GetGameTick() })
        return getmetatable(self).OnFire(self)
    end
    local seen, fired = 0, false
    for _ = 1, 100 do
        WaitTicks(1)
        local now = GetGameTick()
        for i = seen + 1, table.getn(events) do
            local e = events[i]
            check(e[2] == now, string.format('%s read tick %d, the threads of its beat %d', e[1], e[2], now))
            if e[1] == 'OnFire' then fired = true end
        end
        seen = table.getn(events)
        if fired then break end
    end
    check(fired, 'the tank fired')
    note(string.format('%d weapon callbacks checked', seen))
    ScenarioResult.done = true
end)
