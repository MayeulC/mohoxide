-- Scenario test (tests/scenarios.rs): ferry beacons (`AI.BeaconName`, moho's
-- `CUnitFerryTask`). Two UEF T2 transports given one ferry order: the first
-- to start it makes one UEB5102 beacon for its army at the route's point and
-- hears OnFerryPointSet; the second uses the same beacon, without the
-- callback. A T1 tank (no BeaconName) on a ferry order makes nothing and hears
-- nothing. The beacon goes when the order ends.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function beacons()
    local n, at = 0, nil
    for _, u in GetUnitsInRect(Rect(0, 0, 1024, 1024)) or {} do
        if u:GetUnitId() == 'ueb5102' and not u:IsDead() then n = n + 1 at = u end
    end
    return n, at
end
ForkThread(function()
    WaitTicks(2)
    local a = CreateUnitHPR('uea0104', 'ARMY_1', 400, GetTerrainHeight(400, 600) + 10, 600, 0, 0, 0)
    local b = CreateUnitHPR('uea0104', 'ARMY_1', 410, GetTerrainHeight(410, 600) + 10, 600, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 380, GetTerrainHeight(380, 620), 620, 0, 0, 0)
    local heard = {}
    for _, u in {a, b, tank} do
        u.OnFerryPointSet = function(self) heard[self:GetEntityId()] = (heard[self:GetEntityId()] or 0) + 1 end
    end
    IssueFerry({a, b}, {450, GetTerrainHeight(450, 640), 640})
    IssueFerry({tank}, {360, GetTerrainHeight(360, 640), 640})
    check(heard[a:GetEntityId()] == nil, 'no callback at issue time (moho: when the order starts)')
    WaitTicks(2)
    local n, beacon = beacons()
    note(string.format('beacons %d; heard a %s b %s tank %s', n, tostring(heard[a:GetEntityId()]), tostring(heard[b:GetEntityId()]), tostring(heard[tank:GetEntityId()])))
    check(n == 1, 'one beacon for the shared ferry order: ' .. n)
    if beacon then
        local p = beacon:GetPosition()
        check(VDist2(p[1], p[3], 450, 640) < 0.5, 'at the route point')
        check(beacon:GetArmy() == a:GetArmy(), "in the transport's army")
        check(a:GetTransportFerryBeacon() == beacon and b:GetTransportFerryBeacon() == beacon, 'both transports report it')
    end
    local calls = (heard[a:GetEntityId()] or 0) + (heard[b:GetEntityId()] or 0)
    check(calls == 1, 'OnFerryPointSet once, from the transport that made it: ' .. calls)
    check(heard[tank:GetEntityId()] == nil, 'no BeaconName: no callback')
    check(tank:GetTransportFerryBeacon() == nil, 'and no beacon')
    -- The order ends (cleared): its beacon goes with it.
    IssueClearCommands({a, b})
    WaitTicks(2)
    check(beacons() == 0, 'the beacon goes with its order')
    ScenarioResult.done = true
end)
