-- Differential probe: land unit movement and crowd behaviour, logged every
-- tick in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_land 150
--   ours: cargo run -- --headless --no-ai --ticks 620 --script dev/probes/maps/probe_land/probe.lua
-- Per unit: position, facing, velocity (so slip = sideways speed against
-- facing), idle flag. Scenarios sit on the two big flat areas of Seton's;
-- the armies' start units are cleared first (the map script sets sandbox
-- victory so that doesn't end the game).
-- Area A (x 600-790, z 70-220):
--   line: one uel0201 drives 70 straight east.
--   behind<d>: one uel0201 facing east ordered to a point d straight behind.
--   side / diag: one uel0201 ordered 8 to its left / 135 degrees back left.
--   uturn: one uel0201 east, then ordered 45 back behind it at tick 30.
--   clump: 9 uel0201 (3x3, 3 apart) moved to one point.
--   pile / pile10: 6 / 10 uel0201 spawned on top of each other, no orders.
--   block: one uel0201 drives through 9 idle uel0201.
--   big: a Monkeylord walks through 6 idle uel0106.
-- Area B (x 200-390, z 790-950):
--   head: 5 uel0201 east and 5 west, head-on in the same lanes.
--   cross: a column of 5 uel0201 east crosses a column of 5 going north.
--   mixed: 4 uel0101 + 4 uel0201 + 2 uel0303 moved (no formation) north.
--   fight: 5 uel0201 aggressive-moved into 5 enemy uel0201.
local TICKS = 600
local armies = ListArmies()
local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

-- The armies' start units (commanders) would fight and build in the areas.
local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local all, ids = {}, {}
local function spawn(tag, bp, x, z, yaw, army)
    local u = CreateUnitHPR(bp, army or armies[1], x, GetTerrainHeight(x, z), z, 0, yaw or 0, 0)
    table.insert(all, { tag = tag, u = u })
    ids[u:GetEntityId()] = true
    return u
end
local E, N, W = math.pi / 2, 0, -math.pi / 2

local orders = {}
local function move(units, x, z) table.insert(orders, function() IssueMove(units, { x, 0, z }) end) end

move({ spawn('line', 'uel0201', 610, 80, E) }, 680, 80)
for i, d in { 4, 6, 8, 12, 20 } do move({ spawn('behind' .. d, 'uel0201', 650, 92 + i * 8, E) }, 650 - d, 92 + i * 8) end
move({ spawn('side', 'uel0201', 620, 100, E) }, 620, 108)
move({ spawn('diag', 'uel0201', 620, 125, E) }, 614, 131)
local uturn = spawn('uturn', 'uel0201', 610, 160, E)
move({ uturn }, 660, 160)
local clump = {}
for i = 0, 8 do table.insert(clump, spawn('clump', 'uel0201', 700 + math.mod(i, 3) * 3, 84 + math.floor(i / 3) * 3, E)) end
move(clump, 760, 87)
for i = 0, 5 do spawn('pile', 'uel0201', 760 + math.mod(i, 2) * 0.4, 150 + math.floor(i / 2) * 0.4, E) end
for i = 0, 9 do spawn('pile10', 'uel0201', 720 + math.mod(i, 3) * 0.3, 150 + math.floor(i / 3) * 0.3, E) end
move({ spawn('block', 'uel0201', 700, 203, E) }, 780, 203)
for i = 0, 8 do spawn('block', 'uel0201', 737 + math.mod(i, 3) * 3, 200 + math.floor(i / 3) * 3, E) end
move({ spawn('big', 'url0402', 610, 182, E) }, 700, 182)
for i = 0, 5 do spawn('big', 'uel0106', 650 + math.mod(i, 2) * 3, 177 + math.floor(i / 2) * 4, E) end

local east, west = {}, {}
for i = 0, 4 do
    table.insert(east, spawn('head', 'uel0201', 200, 795 + i * 3, E))
    table.insert(west, spawn('head', 'uel0201', 290, 795 + i * 3, W))
end
move(east, 290, 801)
move(west, 200, 801)
local ce, cn = {}, {}
for i = 0, 4 do
    table.insert(ce, spawn('cross', 'uel0201', 220, 857 + i * 3, E))
    table.insert(cn, spawn('cross', 'uel0201', 268 + i * 3, 822, N))
end
move(ce, 320, 863)
move(cn, 274, 910)
local mixed = {}
for i = 0, 3 do table.insert(mixed, spawn('mixed', 'uel0101', 350 + i * 3, 800, N)) end
for i = 0, 3 do table.insert(mixed, spawn('mixed', 'uel0201', 350 + i * 3, 805, N)) end
for i = 0, 1 do table.insert(mixed, spawn('mixed', 'uel0303', 352 + i * 5, 811, N)) end
move(mixed, 360, 900)
local ours, theirs = {}, {}
for i = 0, 4 do
    table.insert(ours, spawn('fight', 'uel0201', 220, 928 + i * 3, E))
    table.insert(theirs, spawn('fight', 'uel0201', 300, 928 + i * 3, W, armies[2]))
end
table.insert(orders, function()
    IssueAggressiveMove(ours, { 300, 0, 934 })
    IssueAggressiveMove(theirs, { 220, 0, 934 })
end)

-- Facing (yaw) from the orientation quaternion {x, y, z, w}.
local function yaw(q)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    local fx, fz = 2 * (qx * qz + qw * qy), 1 - 2 * (qx * qx + qy * qy)
    return math.atan2(fx, fz)
end

local function sample(t)
    for i, s in all do
        local u = s.u
        if u:BeenDestroyed() or u.Dead then
            LOG('PROBE ' .. t .. ' ' .. s.tag .. ' ' .. i .. ' gone')
        else
            local x, _, z = u:GetPositionXYZ()
            local vx, _, vz = u:GetVelocity()
            LOG('PROBE ' .. t .. ' ' .. s.tag .. ' ' .. i .. ' ' .. f2(x) .. ' ' .. f2(z) .. ' ' .. f3(yaw(u:GetOrientation()))
                .. ' ' .. f3(10 * vx) .. ' ' .. f3(10 * vz) .. ' ' .. (u:IsIdleState() and 'i' or 'b'))
        end
    end
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    for _, o in orders do o() end
    -- Ticks since the orders (WaitTicks(n) waits n - 1 ticks in the real
    -- game, so the loop reads the game tick).
    local start, turned = GetGameTick(), false
    LOG('PROBE info start=' .. start)
    while GetGameTick() - start <= TICKS do
        local t = GetGameTick() - start
        if t >= 30 and not turned then
            turned = true
            IssueClearCommands({ uturn })
            IssueMove({ uturn }, { 615, 0, 160 })
        end
        if math.mod(t, 10) == 0 then sweep(ids) end
        sample(t)
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
