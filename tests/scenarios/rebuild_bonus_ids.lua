-- Scenario test (tests/scenarios.rs): which wrecks a build rebuilds is the
-- new blueprint's Economy.RebuildBonusIds, not its own id: a T1 radar on a
-- T2 radar's wreck rebuilds it (moho asks GetRebuildBonus, the wreck isn't
-- reclaimed first); a power generator whose list is cleared reclaims its
-- own wreck first.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function case(wreck_bp, build, x, z)
    local pos = { x, GetTerrainHeight(x, z), z }
    import('/lua/wreckage.lua').CreateWreckage(__blueprints[wreck_bp], pos, { 0, 0, 0, 1 }, 50, 0, 1)
    WaitTicks(1)
    local r = { asked = 0, reclaimed = false }
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', x - 8, GetTerrainHeight(x - 8, z), z, 0, 0, 0)
    local orig = eng.GetRebuildBonus
    eng.GetRebuildBonus = function(self, b) r.asked = r.asked + 1 return orig(self, b) end
    eng.OnStartReclaim = function(self, t) r.reclaimed = true end
    IssueBuildMobile({ eng }, pos, build, {})
    for _ = 1, 300 do
        WaitTicks(1)
        if eng:GetFocusUnit() and not eng:IsUnitState('Reclaiming') then break end
    end
    return r
end
ForkThread(function()
    WaitTicks(2)
    local b = GetArmyBrain('ARMY_1')
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local radar = case('ueb3201', 'ueb3101', 410.5, 600.5)
    check(radar.asked == 1, 'T1 radar on a T2 radar wreck: GetRebuildBonus asked ' .. radar.asked)
    check(not radar.reclaimed, 'T1 radar on a T2 radar wreck: not reclaimed first')
    __blueprints['ueb1101'].Economy.RebuildBonusIds = nil
    local pgen = case('ueb1101', 'ueb1101', 430.5, 580.5)
    check(pgen.asked == 0, 'no list: GetRebuildBonus not asked (' .. pgen.asked .. ')')
    check(pgen.reclaimed, 'no list: its own wreck is reclaimed first')
    ScenarioResult.done = true
end)
