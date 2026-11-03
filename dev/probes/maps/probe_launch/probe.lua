-- Differential probe: aircraft leaving an Atlantis, logged in a
-- format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_launch 150
--   ours: cargo run -- --headless --no-ai --ticks 900 --script dev/probes/maps/probe_launch/probe.lua
-- Three cases, each with interceptors put in storage (AddUnitToStorage):
--   unload: 8 planes, the surfaced carrier ordered to unload 30 away;
--   move:   4 planes, the stored planes themselves ordered to move;
--   sub:    4 planes, the carrier submerged, then ordered to unload.
-- Logged: the attach / launch bones (relative to the carrier), and per tick
-- the carrier, which planes have left, and for each plane its first 6
-- ticks out: position relative to the carrier, yaw / pitch / roll, velocity
-- (per second).
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local function f(v) return string.format('%.2f', v) end
local function f3(v) return string.format('%.3f', v) end
local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
local site
for z = 100, 924, 8 do
    for x = 100, 924, 8 do
        if not site and depth(x, z) > 10 and depth(x + 40, z) > 10 and depth(x + 20, z) > 10 then site = { x, z } end
    end
end
local sx, sz = site[1], site[2]
local carrier = CreateUnitHPR('ues0401', armies[1], sx, GetSurfaceHeight(sx, sz), sz, 0, 0, 0)
local start = 0
local function t() return GetGameTick() - start end

-- Quaternion {x, y, z, w} applied to v.
local function rotate(q, v)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    local tx = 2 * (qy * v[3] - qz * v[2])
    local ty = 2 * (qz * v[1] - qx * v[3])
    local tz = 2 * (qx * v[2] - qy * v[1])
    return { v[1] + qw * tx + (qy * tz - qz * ty), v[2] + qw * ty + (qz * tx - qx * tz), v[3] + qw * tz + (qx * ty - qy * tx) }
end
local function rel(p)
    local c = carrier:GetPosition()
    return f(p[1] - c[1]) .. ',' .. f(p[2] - c[2]) .. ',' .. f(p[3] - c[3])
end
local function pose(u)
    if u:BeenDestroyed() or u.Dead then return 'gone' end
    local q = u:GetOrientation()
    local fw = rotate(q, { 0, 0, 1 })
    local up = rotate(q, { 0, 1, 0 })
    local r = rotate(q, { 1, 0, 0 })
    local vx, vy, vz = u:GetVelocity()
    return rel(u:GetPosition()) .. ' layer=' .. u:GetCurrentLayer()
        .. ' yaw=' .. f3(math.atan2(fw[1], fw[3])) .. ' pitch=' .. f3(math.asin(math.max(-1, math.min(1, fw[2]))))
        .. ' roll=' .. f3(math.atan2(-r[2], up[2]))
        .. ' v=' .. f(10 * vx) .. ',' .. f(10 * vy) .. ',' .. f(10 * vz)
end
local function carrier_state()
    local s = carrier:GetCurrentLayer()
    for _, n in { 'MovingUp', 'MovingDown', 'TransportUnloading' } do
        if carrier:IsUnitState(n) then s = s .. '+' .. n end
    end
    local p = carrier:GetPosition()
    return s .. ' cargo ' .. table.getn(carrier:GetCargo()) .. ' queue ' .. table.getn(carrier:GetCommandQueue())
        .. ' at ' .. f(p[1] - sx) .. ',' .. f(p[2] - GetSurfaceHeight(p[1], p[3])) .. ',' .. f(p[3] - sz)
end
local function bones(phase)
    for i = 0, carrier:GetBoneCount() - 1 do
        local name = carrier:GetBoneName(i)
        local l = string.lower(name)
        if string.find(l, 'attachpoint') or string.find(l, 'launchpoint') then
            local d = ''
            if carrier.GetBoneDirection then
                local ok, x, y, z = pcall(carrier.GetBoneDirection, carrier, i)
                if ok and x then d = ' dir=' .. f3(x) .. ',' .. f3(y) .. ',' .. f3(z) end
            end
            LOG('PROBE ' .. phase .. ' bone ' .. i .. ' ' .. name .. ' ' .. rel(carrier:GetPosition(i)) .. d)
        end
    end
end
local function store(n)
    local planes = {}
    for i = 1, n do
        local p = CreateUnitHPR('uea0102', armies[1], sx + 10 + i * 3, GetSurfaceHeight(sx, sz) + 15, sz + 6, 0, 0, 0)
        carrier:AddUnitToStorage(p)
        table.insert(planes, p)
    end
    return planes
end
local function in_cargo(u)
    for _, c in carrier:GetCargo() do if c == u then return true end end
    return false
end
-- Per tick for `ticks`: the carrier when it changes, each plane's leaving
-- tick and its first 6 ticks out.
local function watch(phase, planes, ticks)
    local out, last = {}, nil
    for i = 0, ticks do
        local s = carrier_state()
        if s ~= last then LOG('PROBE ' .. phase .. ' ' .. i .. ' carrier ' .. s) last = s end
        for n, p in planes do
            if not out[n] and not p:BeenDestroyed() and not in_cargo(p) then
                out[n] = i
                LOG('PROBE ' .. phase .. ' ' .. i .. ' plane ' .. n .. ' left')
            end
            if out[n] and i - out[n] <= 6 then
                LOG('PROBE ' .. phase .. ' ' .. i .. ' plane ' .. n .. ' +' .. (i - out[n]) .. ' ' .. pose(p))
            end
        end
        WaitTicks(1)
    end
    for _, p in planes do if not p:BeenDestroyed() then p:Destroy() end end
    WaitTicks(2)
end

ForkThread(function()
    WaitTicks(40)
    start = GetGameTick()
    bones('unload')
    local planes = store(8)
    WaitTicks(1)
    for n, p in planes do LOG('PROBE unload stored plane ' .. n .. ' ' .. pose(p)) end
    WaitTicks(4)
    IssueTransportUnload({ carrier }, { sx + 30, GetSurfaceHeight(sx + 30, sz), sz })
    watch('unload', planes, 40)

    planes = store(4)
    WaitTicks(5)
    IssueMove(planes, { sx + 30, GetSurfaceHeight(sx + 30, sz) + 20, sz })
    WaitTicks(1)
    for n, p in planes do LOG('PROBE move queued plane ' .. n .. ' queue ' .. table.getn(p:GetCommandQueue()) .. ' attached ' .. tostring(p:IsUnitState('Attached'))) end
    watch('move', planes, 60)

    planes = store(4)
    IssueDive({ carrier })
    for _ = 1, 200 do
        if carrier:GetCurrentLayer() == 'Sub' and not carrier:IsUnitState('MovingDown') then break end
        WaitTicks(1)
    end
    WaitTicks(5)
    LOG('PROBE sub dived ' .. carrier_state())
    IssueTransportUnload({ carrier }, { sx + 30, GetSurfaceHeight(sx + 30, sz), sz })
    watch('sub', planes, 120)
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
