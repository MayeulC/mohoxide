-- Differential probe: land formations, logged every tick in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_landform 150
--   ours: cargo run -- --headless --no-ai --ticks 720 --script dev/probes/maps/probe_landform/probe.lua
-- Per unit: position, facing, velocity (so slip = sideways speed against
-- facing), idle flag. Scenarios sit on the two big flat areas of Seton's;
-- the armies' start units are cleared first (the map script sets sandbox
-- victory so that doesn't end the game).
-- Area A (x 600-790, z 70-220):
--   attack: 12 uel0201 in AttackFormation 90 east.
--   growth: the same in GrowthFormation.
--   reform: 12 uel0201 spread 6 apart form-moved onto their own middle.
--   turn: 12 uel0201 form-moved 70 east, then (queued) 30 north.
-- Area B (x 200-390, z 790-950):
--   mix: 12 uel0201 + 4 uel0101 + 4 uel0103 in AttackFormation 100 east.
--   rev: 12 uel0201 form-moved 60 east, then (queued) back 60 west.
local TICKS = 700
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

local attack = block('attack', 610, 76, E)
form(attack, 700, 79, 'AttackFormation', 90)
local growth = block('growth', 610, 126, E)
form(growth, 700, 129, 'GrowthFormation', 90)
local reform = block('reform', 742, 80, E, 12, 6)
form(reform, 751, 86, 'AttackFormation', 90)
local turn = block('turn', 610, 180, E)
form(turn, 680, 183, 'AttackFormation', 90)
form(turn, 680, 213, 'AttackFormation', 0)
local mix = block('mix', 200, 850, E)
for i = 0, 3 do table.insert(mix, spawn('mix', 'uel0101', 212, 850 + i * 3, E)) end
for i = 0, 3 do table.insert(mix, spawn('mix', 'uel0103', 215, 850 + i * 3, E)) end
form(mix, 300, 856, 'AttackFormation', 90)
local rev = block('rev', 200, 905, E)
form(rev, 260, 908, 'AttackFormation', 90)
form(rev, 200, 908, 'AttackFormation', -90)

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
