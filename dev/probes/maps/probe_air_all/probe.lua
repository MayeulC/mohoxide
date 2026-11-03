-- Differential probe: every air unit flies the same course, logged per tick
-- in a format both engines print identically ("PROBE ..." lines). Engine APIs
-- only (Lua 5.0 safe), so it runs on FAF's ForgedAlliance.exe as well as ours.
--   real game (Windows): dev/probes/run_faf.ps1 probe_air_all 240
--   ours: cargo run -- --headless --no-ai --ticks 470 --script dev/probes/maps/probe_air_all/probe.lua
-- Each unit (faction air units: ids like uea0303, sorted) gets a lane 13
-- apart, starts at rest facing east, and at tick 20 is sent 350 east with a
-- queued leg 80 north (a corner at speed); at tick 200 it's ordered back west
-- (a reversal at speed); from about tick 330 it's idle.
local TICKS = 450

local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

local armies = ListArmies()
local w, h = GetMapSize()

local ids = {}
for id, bp in __blueprints do
    if type(id) == 'string' and string.find(id, '^%a%aa%d%d%d%d$') and bp.Physics and bp.Physics.MotionType == 'RULEUMT_Air' then
        table.insert(ids, id)
    end
end
table.sort(ids)

local function rotate(q, v)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    local tx = 2 * (qy * v[3] - qz * v[2])
    local ty = 2 * (qz * v[1] - qx * v[3])
    local tz = 2 * (qx * v[2] - qy * v[1])
    return { v[1] + qw * tx + (qy * tz - qz * ty), v[2] + qw * ty + (qz * tx - qx * tz), v[3] + qw * tz + (qx * ty - qy * tx) }
end

local function state(u)
    if u:BeenDestroyed() or u.Dead then return 'gone' end
    local x, y, z = u:GetPositionXYZ()
    local q = u:GetOrientation()
    local f = rotate(q, { 0, 0, 1 })
    local up = rotate(q, { 0, 1, 0 })
    local r = rotate(q, { 1, 0, 0 })
    local vx, vy, vz = u:GetVelocity()
    return f2(x) .. ' ' .. f2(y) .. ' ' .. f2(z) .. ' h=' .. f2(y - GetTerrainHeight(x, z))
        .. ' yaw=' .. f3(math.atan2(f[1], f[3])) .. ' pitch=' .. f3(math.asin(math.max(-1, math.min(1, f[2]))))
        .. ' roll=' .. f3(math.atan2(-r[2], up[2]))
        .. ' v=' .. f2(10 * math.sqrt(vx * vx + vz * vz)) .. ' vy=' .. f2(10 * vy)
        .. ' layer=' .. u:GetCurrentLayer()
end

local x0, z0, gap = 150, 40, 13
local units, lanes = {}, {}
for i, id in ids do
    local z = z0 + (i - 1) * gap
    if z > h - 40 then break end
    local ok, u = pcall(CreateUnitHPR, id, armies[1], x0, GetTerrainHeight(x0, z) + 20, z, 0, math.pi / 2, 0)
    if ok and u then
        table.insert(units, u)
        table.insert(lanes, { id = id, z = z })
    else
        LOG('PROBE spawn failed ' .. id)
    end
end
LOG('PROBE units ' .. table.getn(units))
for i, l in lanes do LOG('PROBE lane ' .. i .. ' ' .. l.id .. ' ' .. l.z) end

local function ground(x, z) return { x, GetTerrainHeight(x, z), z } end

ForkThread(function()
    local start = GetGameTick()
    for t = 0, TICKS do
        if t == 20 then
            for i, u in units do
                if not u:BeenDestroyed() then
                    IssueMove({ u }, ground(x0 + 350, lanes[i].z))
                    IssueMove({ u }, ground(x0 + 350, lanes[i].z + 80))
                end
            end
        elseif t == 200 then
            for i, u in units do
                if not u:BeenDestroyed() then
                    IssueClearCommands({ u })
                    IssueMove({ u }, ground(x0 + 100, lanes[i].z))
                end
            end
        end
        local at = 'PROBE t=' .. (GetGameTick() - start) .. ' '
        for i, u in units do LOG(at .. i .. ' ' .. state(u)) end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
