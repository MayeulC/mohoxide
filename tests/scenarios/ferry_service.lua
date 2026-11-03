-- Scenario test (tests/scenarios.rs): a ferry route carries the units
-- waiting at its beacon (moho's `CUnitFerryTask` 0x101fa160 and
-- `CUnitWaitForFerryTask` 0x101fa920). A UEF T1 transport is given a route
-- of two ferry points (its beacon at the first); two tanks ordered onto the
-- beacon wait there (`WaitForFerry`), the transport picks them up, flies the
-- route, unloads them at its last point, their orders end, and it heads back
-- to the beacon. A third tank waiting when the route is cleared: the beacon
-- goes with the order, and the tank's order ends with it.
-- (The third tank is sent by its platoon: `CPlatoon:UseFerryBeacon`.)
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function ground(x, z) return GetTerrainHeight(x, z) end
ForkThread(function()
    WaitTicks(2)
    local t = CreateUnitHPR('uea0107', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local tanks = {}
    for i = 1, 2 do
        table.insert(tanks, CreateUnitHPR('uel0201', 'ARMY_1', 404 + i * 3, ground(404 + i * 3, 612), 612, 0, 0, 0))
    end
    local attached = 0
    for _, tank in tanks do
        local base = tank.OnAttachedToTransport
        tank.OnAttachedToTransport = function(self, transport, bone)
            attached = attached + 1
            if base then base(self, transport, bone) end
        end
    end
    IssueFerry({t}, {410, ground(410, 610), 610})
    IssueFerry({t}, {470, ground(470, 640), 640})
    WaitTicks(2)
    local beacon = t:GetTransportFerryBeacon()
    check(beacon ~= nil, 'the route has its beacon')
    if not beacon then ScenarioResult.done = true return end
    IssueTransportLoad(tanks, beacon)
    WaitTicks(3)
    for _, tank in tanks do
        check(tank:IsUnitState('WaitForFerry'), 'a tank waits for the ferry')
    end
    -- Picked up, flown out and set down near the route's last point.
    local delivered = false
    for _ = 1, 120 do
        WaitTicks(10)
        delivered = true
        for _, tank in tanks do
            local p = tank:GetPosition()
            if tank:IsUnitState('Attached') or VDist2(p[1], p[3], 470, 640) > 12 then delivered = false end
        end
        if delivered then break end
    end
    note(string.format('attached %d; transport at %s', attached, repr(t:GetPosition())))
    check(attached == 2, 'both boarded: ' .. attached)
    check(delivered, 'both set down at the route end')
    for _, tank in tanks do
        check(not tank:IsUnitState('WaitForFerry'), 'not waiting any more')
        check(tank:IsIdleState(), 'its order over once off the transport')
    end
    -- The transport goes back to its beacon and keeps ferrying.
    local back = false
    for _ = 1, 80 do
        WaitTicks(10)
        local p = t:GetPosition()
        if VDist2(p[1], p[3], 410, 610) < 4 then back = true break end
    end
    check(back, 'back at the beacon')
    check(t:IsUnitState('Ferrying'), 'still ferrying')
    -- A tank waiting when the route is cleared (sent by its platoon,
    -- `CPlatoon:UseFerryBeacon` as FAF's ScenarioPlatoonAI does): the beacon
    -- goes, the wait ends.
    local late = CreateUnitHPR('uel0201', 'ARMY_1', 380, ground(380, 600), 600, 0, 0, 0)
    local brain = ArmyBrains[1]
    local platoon = brain:MakePlatoon('', '')
    brain:AssignUnitsToPlatoon(platoon, { late }, 'Attack', 'None')
    local cmd = platoon:UseFerryBeacon(categories.ALLUNITS, beacon)
    WaitTicks(3)
    check(late:IsUnitState('WaitForFerry'), 'the late tank waits')
    check(platoon:IsCommandsActive(cmd), 'the platoon command is active while it waits')
    IssueClearCommands({t})
    WaitTicks(3)
    check(beacon:BeenDestroyed(), 'the beacon goes with the route')
    check(late:IsIdleState() and not late:IsUnitState('WaitForFerry'), 'the wait ends with the beacon')
    check(not platoon:IsCommandsActive(cmd), 'and the platoon command')
    ScenarioResult.done = true
end)
