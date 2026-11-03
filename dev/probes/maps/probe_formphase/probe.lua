-- Differential probe: land formations, logged every tick in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_formphase 150
--   ours: cargo run -- --headless --no-ai --ticks 620 --script dev/probes/maps/probe_formphase/probe.lua
-- Per unit: position, facing, velocity (so slip = sideways speed against
-- facing), idle flag. Scenarios sit on the two big flat areas of Seton's;
-- the armies' start units are cleared first (the map script sets sandbox
-- victory so that doesn't end the game).
-- What starts the members' speed control (first ~100 ticks every member runs
-- at 0.85 x the formation's speed): time, distance, or being formed up.
--   p1: 12 uel0201 block (4 x 3) 90 east (as probe_landform's attack).
--   p2: 12 uel0101 (faster) block 160 east.
--   p3: 12 uel0201 already in a line across, 90 east.
--   p4: 12 uel0201 block 180 east.
--   p5: 4 uel0201 block (2 x 2) 90 east.
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
local function block(tag, x, z, yaw, n, gap)
    local list = {}
    for i = 0, (n or 12) - 1 do
        table.insert(list, spawn(tag, 'uel0201', x + math.mod(i, 4) * (gap or 3), z + math.floor(i / 4) * (gap or 3), yaw))
    end
    return list
end
local function form(units, x, z, formation, deg) table.insert(orders, function() IssueFormMove(units, { x, 0, z }, formation, deg) end) end

local function blockof(tag, bp, x, z, n, cols)
    local list = {}
    for i = 0, n - 1 do table.insert(list, spawn(tag, bp, x + math.mod(i, cols) * 3, z + math.floor(i / cols) * 3, E)) end
    return list
end
local p1 = blockof('p1', 'uel0201', 610, 76, 12, 4)
form(p1, 700, 79, 'AttackFormation', 90)
local p3 = {}
for i = 0, 11 do table.insert(p3, spawn('p3', 'uel0201', 615, 95 + i * 1.9, E)) end
form(p3, 705, 105, 'AttackFormation', 90)
local p5 = blockof('p5', 'uel0201', 610, 140, 4, 2)
form(p5, 700, 141, 'AttackFormation', 90)
local p2 = blockof('p2', 'uel0101', 200, 798, 12, 4)
form(p2, 360, 801, 'AttackFormation', 90)
local p4 = blockof('p4', 'uel0201', 200, 860, 12, 4)
form(p4, 380, 863, 'AttackFormation', 90)

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
    local start = GetGameTick()
    LOG('PROBE info start=' .. start)
    while GetGameTick() - start <= TICKS do
        local t = GetGameTick() - start
        if math.mod(t, 10) == 0 then sweep(ids) end
        sample(t)
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
