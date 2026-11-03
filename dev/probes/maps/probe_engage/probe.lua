-- Differential probe: how attack-moving tanks engage, logged
-- every tick in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_engage 120
--   ours: cargo run -- --headless --no-ai --ticks 420 --script dev/probes/maps/probe_engage/probe.lua
-- Per unit: position, facing, speed, health, its weapon's target (probe
-- index, '-' for none), Attacking / Moving states, the navigator's current
-- target position. Once at the start: each unit's muzzle above its origin.
-- Scenarios (uel0201 throughout):
--   fight: probe_land's fight, 5 aggressive-moved into 5 enemies (x 220 / 300,
--          z 928-940 on Seton's: a small crest lies between lanes 1-3).
--   duel: 1 aggressive-moved into 1 enemy aggressive-moved back, on the flat
--         area A (z 120).
--   idle: 1 aggressive-moved at 1 enemy that has no order (z 140).
--   idle2: the same with the armies swapped (z 205).
--   still: 2 enemies 15 apart, no orders (z 100).
-- Also per unit: whether its army sees its opponent now (blip), 'S' / '.'.
local TICKS = 400
local armies = ListArmies()
local function f2(n) return string.format('%.2f', n) end

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local all, ids, index = {}, {}, {}
local function spawn(tag, x, z, yaw, army)
    local u = CreateUnitHPR('uel0201', army or armies[1], x, GetTerrainHeight(x, z), z, 0, yaw or 0, 0)
    table.insert(all, { tag = tag, u = u })
    ids[u:GetEntityId()] = true
    index[u:GetEntityId()] = table.getn(all)
    return u
end
local E, W = math.pi / 2, -math.pi / 2

local ours, theirs = {}, {}
for i = 0, 4 do
    table.insert(ours, spawn('fight', 220, 928 + i * 3, E))
    table.insert(theirs, spawn('fight', 300, 928 + i * 3, W, armies[2]))
end
local d1, d2 = spawn('duel', 620, 120, E), spawn('duel', 700, 120, W, armies[2])
local i1 = spawn('idle', 640, 140, E)
spawn('idle', 720, 140, W, armies[2])
local i2 = spawn('idle2', 640, 205, E, armies[2])
spawn('idle2', 720, 205, W)
spawn('still', 650, 100, E)
spawn('still', 665, 100, W, armies[2])
-- Each unit's opponent: the other unit of its pair.
local function opponent(i) return math.mod(i, 2) == 1 and i + 1 or i - 1 end

local function yaw(q)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    return math.atan2(2 * (qx * qz + qw * qy), 1 - 2 * (qx * qx + qy * qy))
end

local function target_of(u)
    local ok, t = pcall(function() return u:GetWeapon(1):GetCurrentTarget() end)
    if not ok or not t then return '-' end
    local ok2, id = pcall(function() return t:GetEntityId() end)
    if not ok2 then return '?' end
    return tostring(index[id] or ('e' .. id))
end

local function nav_pos(u)
    local ok, p = pcall(function() return u:GetNavigator():GetCurrentTargetPos() end)
    if not ok or not p then return '- -' end
    return f2(p[1]) .. ' ' .. f2(p[3])
end

local function seen(i, u)
    local o = all[opponent(i)].u
    if o:BeenDestroyed() then return '-' end
    local army = u:GetArmy()
    local ok, r = pcall(function() local b = o:GetBlip(army) return b and b:IsSeenNow(army) end)
    if not ok then return '?' end
    return r and 'S' or '.'
end

local function sample(t)
    for i, s in all do
        local u = s.u
        if u:BeenDestroyed() or u.Dead then
            LOG('PROBE ' .. t .. ' ' .. s.tag .. ' ' .. i .. ' gone')
        else
            local x, _, z = u:GetPositionXYZ()
            local vx, _, vz = u:GetVelocity()
            LOG('PROBE ' .. t .. ' ' .. s.tag .. ' ' .. i .. ' ' .. f2(x) .. ' ' .. f2(z) .. ' ' .. f2(yaw(u:GetOrientation()))
                .. ' ' .. f2(10 * math.sqrt(vx * vx + vz * vz)) .. ' ' .. math.floor(u:GetHealth())
                .. ' ' .. target_of(u) .. ' ' .. (u:IsUnitState('Attacking') and 'A' or '.') .. (u:IsUnitState('Moving') and 'M' or '.')
                .. ' ' .. nav_pos(u) .. ' ' .. seen(i, u))
        end
    end
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    for i, s in all do
        local u = s.u
        local x, y, z = u:GetPositionXYZ()
        local m = u:GetPosition('Turret_Muzzle')
        LOG('PROBE info muzzle ' .. i .. ' ' .. f2(m[1] - x) .. ' ' .. f2(m[2] - y) .. ' ' .. f2(m[3] - z)
            .. ' ground ' .. f2(y - GetTerrainHeight(x, z)))
    end
    IssueAggressiveMove(ours, { 300, 0, 934 })
    IssueAggressiveMove(theirs, { 220, 0, 934 })
    IssueAggressiveMove({ d1 }, { 700, 0, 120 })
    IssueAggressiveMove({ d2 }, { 620, 0, 120 })
    IssueAggressiveMove({ i1 }, { 720, 0, 140 })
    IssueAggressiveMove({ i2 }, { 720, 0, 205 })
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
