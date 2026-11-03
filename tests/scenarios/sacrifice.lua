-- Scenario test (tests/scenarios.rs): an Aeon engineer ordered to
-- sacrifice into an unfinished structure walks over, gets
-- OnStartSacrifice(target), adds FAF's CalculateSacrificeBonus to its build
-- progress, then OnStopSacrifice(target) destroys it.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    IssueBuildMobile({eng}, {406, GetTerrainHeight(406, 600), 600}, 'ueb1101', {})
    local site
    for _ = 1, 100 do
        WaitTicks(1)
        site = eng:GetFocusUnit()
        if site and site:GetFractionComplete() > 0.05 then break end
    end
    check(site ~= nil, 'engineer started the power generator')
    if not site then ScenarioResult.done = true return end
    IssueClearCommands({eng})
    WaitTicks(2)
    local before = site:GetFractionComplete()
    local aeon = CreateUnitHPR('ual0105', 'ARMY_1', 420, GetTerrainHeight(420, 600), 600, 0, 0, 0)
    local calls = {}
    local start, stop = aeon.OnStartSacrifice, aeon.OnStopSacrifice
    aeon.OnStartSacrifice = function(self, t) table.insert(calls, t == site and 'start' or 'start?') return start(self, t) end
    aeon.OnStopSacrifice = function(self, t) table.insert(calls, t == site and 'stop' or 'stop?') return stop(self, t) end
    local want = site:CalculateSacrificeBonus(aeon)
    WaitTicks(1)
    IssueSacrifice({aeon}, site)
    for _ = 1, 200 do
        WaitTicks(1)
        if calls[2] then break end
    end
    check(table.concat(calls, ' ') == 'start stop', 'callbacks: ' .. table.concat(calls, ' '))
    local gained = site:GetFractionComplete() - before
    check(math.abs(gained - math.min(want, 1 - before)) < 0.01, string.format('progress +%.3f, FAF says %.3f', gained, want))
    WaitTicks(2)
    check(aeon:BeenDestroyed(), 'the sacrificer is gone')
    ScenarioResult.done = true
end)
