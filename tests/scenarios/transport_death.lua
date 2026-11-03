-- Scenario test (tests/scenarios.rs): a transport shot down with cargo.
-- FAF kills the cargo with it; nothing is left alive, attached to the
-- wreck, or under the map, and no detach callback errors.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function ground(x, z) return GetTerrainHeight(x, z) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local t = CreateUnitHPR('uea0107', 'ARMY_1', 400, 40, 600, 0, 0, 0)
        local tanks = {}
        for i = 1, 4 do table.insert(tanks, CreateUnitHPR('uel0201', 'ARMY_1', 390 + i * 3, ground(390 + i * 3, 610), 610, 0, 0, 0)) end
        WaitTicks(10)
        IssueTransportLoad(tanks, t)
        WaitTicks(80)
        check(table.getn(t:GetCargo()) == 4, 'all four aboard, ' .. table.getn(t:GetCargo()))
        IssueMove({ t }, { 460, 0, 600 })
        WaitTicks(20)
        t:Kill()
        WaitTicks(150)
        for i, u in tanks do
            local p = u:GetPosition()
            note(string.format('tank %d: dead %s, destroyed %s, attached %s, %.1f above the ground', i, tostring(u.Dead), tostring(IsDestroyed(u)), tostring(not IsDestroyed(u) and u:IsUnitState('Attached')), p[2] - ground(p[1], p[3])))
            check(u.Dead or IsDestroyed(u), 'tank ' .. i .. ' died with the transport')
            check(IsDestroyed(u) or p[2] > ground(p[1], p[3]) - 1, 'tank ' .. i .. ' not left under the map')
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
