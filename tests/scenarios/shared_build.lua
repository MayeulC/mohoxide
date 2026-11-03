-- Scenario test (tests/scenarios.rs): a build order shared by several
-- engineers (a player's order to every selected engineer; FAF's
-- `IssueBuildAllMobile`). Three engineers queue three T1 power generators:
-- all three get every order, build each generator together (one structure
-- per order, not three), and move on to the next as one when it's done.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local b = ArmyBrains[1]
        b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
        b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
        local engs = {}
        for i = 0, 2 do table.insert(engs, CreateUnitHPR('uel0105', 'ARMY_1', 400 + i * 2, GetTerrainHeight(400, 600), 600, 0, 0, 0)) end
        for i = 0, 2 do
            local x, z = 410.5 + i * 2, 610.5
            IssueBuildAllMobile(engs, { x, GetTerrainHeight(x, z), z }, 'ueb1101', {})
        end
        local function queues() local o = {} for _, e in engs do table.insert(o, table.getn(e:GetCommandQueue())) end return o end
        local q = queues()
        check(q[1] == 3 and q[2] == 3 and q[3] == 3, 'every engineer got all three orders')
        local most_at_once, all_built = 0, nil
        for _ = 1, 600 do
            WaitTicks(1)
            local gens = b:GetListOfUnits(categories.ueb1101, false) or {}
            local unfinished = 0
            for _, g in gens do if g:GetFractionComplete() < 1 then unfinished = unfinished + 1 end end
            most_at_once = math.max(most_at_once, unfinished)
            if table.getn(gens) == 3 and unfinished == 0 then all_built = GetGameTick() break end
        end
        q = queues()
        note(string.format('all three built by tick %s; at most %d under construction at once; queues %d %d %d', tostring(all_built), most_at_once, q[1], q[2], q[3]))
        check(all_built ~= nil, 'all three generators were built')
        check(most_at_once == 1, 'one generator at a time: the engineers build each together')
        check(q[1] == 0 and q[2] == 0 and q[3] == 0, 'and every engineer finished its queue')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
