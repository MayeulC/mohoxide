-- Scenario test (tests/scenarios.rs): an engineer ordered to build over
-- props that obstruct building (a pine tree group: OBSTRUCTSBUILDING)
-- reclaims them first, then builds; the structure doesn't stand in trees.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local blockers = {}
    for _, p in GetReclaimablesInRect(Rect(424, 585, 436, 597)) or {} do
        if p.IsProp and p.Blueprint.CategoriesHash.OBSTRUCTSBUILDING then table.insert(blockers, p) end
    end
    note(string.format('%d obstructing props on the site', table.getn(blockers)))
    check(table.getn(blockers) > 0, 'the test site has a tree group on it')
    local e = CreateUnitHPR('ual0105', 'ARMY_1', 430, GetTerrainHeight(430, 584), 584, 0, 0, 0)
    IssueBuildMobile({ e }, Vector(430, 0, 591), 'uab1101', {})
    local reclaimed, built = false, nil
    for _ = 1, 900 do
        WaitTicks(1)
        if e:IsUnitState('Reclaiming') then reclaimed = true end
        local s = e.UnitBeingBuilt
        if s and not s.Dead then built = s end
        if built and built:GetFractionComplete() >= 1 then break end
    end
    check(reclaimed, 'the engineer reclaimed before building')
    for _, p in blockers do
        local q = p:GetPosition()
        note(string.format('%s at %.1f, %.1f destroyed %s', p.Blueprint.BlueprintId, q[1], q[3], tostring(p:BeenDestroyed())))
    end
    -- The group nearest the site's centre overlaps the footprint.
    local nearest, best
    for _, p in blockers do
        local q = p:GetPosition()
        local d = (q[1] - 430) * (q[1] - 430) + (q[3] - 591) * (q[3] - 591)
        if not best or d < best then nearest, best = p, d end
    end
    check(nearest == nil or nearest:BeenDestroyed(), 'the tree group on the site is gone')
    check(built ~= nil and built:GetFractionComplete() >= 1, 'then built the power generator')
    ScenarioResult.done = true
end)
