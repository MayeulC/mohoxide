-- Scenario test (tests/scenarios.rs): units ease into a stop, as the real
-- game's do. Moho brakes onto the goal from the tick its stopping
-- distance passes what's left and never speeds up again, then, the move
-- done within half a unit, lets the rest of its speed die away (0.8 v less
-- a tick's braking). Ours braked, found itself short and sped up again on
-- the last tick or two: a Seraphim T1 engineer sent 8 went 1.9, 1.52, 1.14,
-- 0.76, 0.38, 0.76, 0.38, 0. The real game's (dev/probes/maps/probe_arrive):
-- 1.9, 1.14, 0.532, 0.046, 0.009, 0. Land, hover and naval units, short and
-- long moves: once a unit's speed starts falling at the end of its move it
-- never rises again.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local movers = {}
    local function add(bp, x, z, d, water)
        local y = water and GetSurfaceHeight(x, z) or GetTerrainHeight(x, z)
        local u = CreateUnitHPR(bp, 'ARMY_1', x, y, z, 0, 0, 0)
        u:SetFireState(1)
        table.insert(movers, { name = bp .. ' ' .. d, u = u, goal = { x, z + d }, speeds = {}, last = { x, z } })
    end
    -- Seton's flat area A (cell centres: the goal isn't moved) and the sea
    -- off the west shore.
    for i, bp in { 'uel0201', 'url0107', 'xsl0105', 'ual0201', 'uel0203', 'uel0303' } do
        add(bp, 600.5 + i * 7, 70.5, 2)
        add(bp, 600.5 + i * 7, 84.5, 8)
        add(bp, 600.5 + i * 7, 98.5, 30)
    end
    for i, bp in { 'ues0103', 'ues0201' } do
        add(bp, 110 + i * 40, 50, 8, true)
        add(bp, 130 + i * 40, 50, 30, true)
    end
    WaitTicks(5)
    for _, m in movers do
        local p = m.u:GetPosition()
        m.last = { p[1], p[3] }
        IssueMove({ m.u }, { m.goal[1], 0, m.goal[2] })
    end
    for _ = 1, 200 do
        WaitTicks(1)
        for _, m in movers do
            local p = m.u:GetPosition()
            local dx, dz = p[1] - m.last[1], p[3] - m.last[2]
            table.insert(m.speeds, math.sqrt(dx * dx + dz * dz) * 10)
            m.last = { p[1], p[3] }
        end
    end
    for _, m in movers do
        local s = m.speeds
        -- From its last tick at its top speed on, the speed only falls.
        local top, from = 0, 1
        for i, v in s do
            if v >= top - 1e-4 then top, from = v, i end
        end
        local rose
        for i = from + 1, table.getn(s) do
            if s[i] > s[i - 1] + 1e-4 then rose = i break end
        end
        local tail = {}
        for i = math.max(from, 1), math.min(from + 6, table.getn(s)) do table.insert(tail, string.format('%.3f', s[i])) end
        check(not rose, string.format('%s: speed rose again stopping (tick %s): %s', m.name, tostring(rose), table.concat(tail, ' ')))
        local p = m.u:GetPosition()
        local dx, dz = p[1] - m.goal[1], p[3] - m.goal[2]
        check(math.sqrt(dx * dx + dz * dz) < 0.5, string.format('%s: stopped within half a unit of its goal (%.3f off)', m.name, math.sqrt(dx * dx + dz * dz)))
        check(s[table.getn(s)] == 0, m.name .. ': at rest')
        table.insert(ScenarioResult.notes, m.name .. ': ' .. table.concat(tail, ' '))
    end
    ScenarioResult.done = true
end)
