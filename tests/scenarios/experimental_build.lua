-- Scenario test (tests/scenarios.rs): mobile experimentals under construction
-- stay put. A T3 engineer per faction starts one (Fatboy, Monkeylord,
-- Galactic Colossus, Ythotha); while each is unfinished its position must not
-- move from where the build started.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 500000) b:__engine_SetStorage('ENERGY', 5000000)
    b:GiveResource('MASS', 500000) b:GiveResource('ENERGY', 5000000)
    local function at(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local jobs = { { 'uel0309', 'uel0401' }, { 'url0309', 'url0402' }, { 'ual0309', 'ual0401' }, { 'xsl0309', 'xsl0401' } }
    for i, j in jobs do
        local x, z = 300 + i * 40, 300
        IssueBuildMobile({ at(j[1], x, z) }, { x + 12, GetTerrainHeight(x + 12, z), z }, j[2], {})
    end
    local start, worst = {}, {}
    for _ = 1, 30 do
        WaitTicks(10)
        for _, u in b:GetListOfUnits(categories.EXPERIMENTAL * categories.MOBILE, false) or {} do
            local id, p = u.UnitId, u:GetPosition()
            if u:GetFractionComplete() < 1 then
                if not start[id] then start[id] = { p[1], p[3] } worst[id] = 0 end
                local d = math.max(math.abs(p[1] - start[id][1]), math.abs(p[3] - start[id][2]))
                if d > worst[id] then worst[id] = d end
            end
        end
    end
    for _, j in jobs do
        local id = j[2]
        check(start[id] ~= nil, id .. ' never started building')
        if start[id] then
            check(worst[id] < 0.01, string.format('%s moved %.3f while being built', id, worst[id]))
            note(string.format('%s drift %.4f', id, worst[id]))
        end
    end
    ScenarioResult.done = true
end)
