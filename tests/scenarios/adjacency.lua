-- Scenario test (tests/scenarios.rs): structure adjacency (moho's
-- OnAdjacentTo / OnNotAdjacentTo). A mass storage two cells from an
-- extractor (their build skirts touch) makes them adjacent and buffs the
-- extractor's production; destroying the storage undoes it. A storage one
-- cell further off isn't adjacent.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function make(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
ForkThread(function()
    WaitTicks(2)
    local ex = make('ueb1103', 400.5, 600.5)
    WaitTicks(5)
    local base = ex:GetProductionPerSecondMass()
    local far = make('ueb1106', 400.5, 603.5)
    WaitTicks(5)
    check(not (ex.AdjacentUnits and ex.AdjacentUnits[far.EntityId]), 'a storage three cells off is not adjacent')
    local st = make('ueb1106', 402.5, 600.5)
    WaitTicks(5)
    local buffed = ex:GetProductionPerSecondMass()
    note(string.format('extractor mass/s alone %.3f, with an adjacent storage %.3f', base, buffed))
    check(ex.AdjacentUnits and ex.AdjacentUnits[st.EntityId] == st, 'the extractor knows the adjacent storage')
    check(st.AdjacentUnits and st.AdjacentUnits[ex.EntityId] == ex, 'and the storage knows the extractor')
    check(buffed > base, 'the storage buffs the extractor')
    st:Destroy()
    WaitTicks(5)
    check(not ex.AdjacentUnits[st.EntityId], 'a destroyed storage is no longer adjacent')
    check(math.abs(ex:GetProductionPerSecondMass() - base) < 1e-6, 'and its buff is gone')
    ScenarioResult.done = true
end)
