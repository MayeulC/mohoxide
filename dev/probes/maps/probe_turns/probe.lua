-- Differential probe: aircraft turning on move orders at speed, logged per
-- tick in a format both engines print identically ("PROBE ..." lines).
-- Engine APIs only, so it runs on FAF's ForgedAlliance.exe as well as ours.
--   real game (Windows): dev/probes/run_faf.ps1 probe_turns
--   ours: cargo run -- --headless --no-ai --ticks 470 --script dev/probes/maps/probe_turns/probe.lua
-- F: a UEF T3 fighter cruising east is ordered north (90 degrees), then
--    back south-west (about 135), each at full speed.
-- G: a UEF T3 fighter flies queued waypoints round a square (90-degree
--    corners on the same order).
-- H: five UEF T3 fighters on one move east, then all ordered back west.
-- T: a UEF T1 transport flies a move with a turn; its engine pods' facing
--    in its own frame is logged (pod= x y z, the front right pod).
-- S: a UEF T2 gunship flies a move with a turn.
local TICKS = 450

local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

local armies = ListArmies()
local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local function spawn(bp, x, z, heading)
    return CreateUnitHPR(bp, armies[1], x, GetTerrainHeight(x, z) + 20, z, 0, heading, 0)
end
local function ground(x, z) return { x, GetTerrainHeight(x, z), z } end

-- Quaternion {x, y, z, w} applied to v, and its inverse.
local function rotate(q, v, inv)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    if inv then qx, qy, qz = -qx, -qy, -qz end
    local tx = 2 * (qy * v[3] - qz * v[2])
    local ty = 2 * (qz * v[1] - qx * v[3])
    local tz = 2 * (qx * v[2] - qy * v[1])
    return { v[1] + qw * tx + (qy * tz - qz * ty), v[2] + qw * ty + (qz * tx - qx * tz), v[3] + qw * tz + (qx * ty - qy * tx) }
end

-- x y z, height over ground, heading / pitch / roll (radians), speed (per second).
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

local function pod(u, bone)
    if u:BeenDestroyed() or u.Dead then return '' end
    local x, y, z = u:GetBoneDirection(bone)
    local d = rotate(u:GetOrientation(), { x, y, z }, true)
    return ' pod=' .. f3(d[1]) .. ' ' .. f3(d[2]) .. ' ' .. f3(d[3])
end

local f = spawn('uea0303', cx - 250, cz - 250, math.pi / 2)
local g = spawn('uea0303', cx - 250, cz - 100, math.pi / 2)
local group = {}
for i = 1, 5 do group[i] = spawn('uea0303', cx - 250, cz + 30 + i * 8, math.pi / 2) end
local t = spawn('uea0107', cx - 250, cz + 200, math.pi / 2)
local s = spawn('uea0203', cx - 250, cz + 300, math.pi / 2)
LOG('PROBE armies ', table.getn(armies), ' map ', w, ' ', h)

ForkThread(function()
    local start = GetGameTick()
    for tick = 0, TICKS do
        if tick == 30 then
            IssueMove({ f }, ground(cx + 250, cz - 250))
            IssueMove({ g }, ground(cx - 100, cz - 100))
            IssueMove({ g }, ground(cx - 100, cz - 250 + 100))
            IssueMove({ g }, ground(cx - 250, cz - 250 + 100))
            IssueMove({ g }, ground(cx - 250, cz - 100))
            IssueMove(group, ground(cx + 250, cz + 60))
            IssueMove({ t }, ground(cx - 100, cz + 200))
            IssueMove({ t }, ground(cx - 100, cz + 100))
            IssueMove({ s }, ground(cx - 100, cz + 300))
            IssueMove({ s }, ground(cx - 100, cz + 400))
        elseif tick == 90 then
            IssueClearCommands({ f })
            IssueMove({ f }, ground(cx - 50, cz - 50))
        elseif tick == 110 then
            IssueClearCommands(group)
            IssueMove(group, ground(cx - 300, cz + 60))
        elseif tick == 200 then
            IssueClearCommands({ f })
            IssueMove({ f }, ground(cx - 300, cz - 300))
        end
        local at = 'PROBE t=' .. (GetGameTick() - start)
        LOG(at .. ' F ' .. state(f))
        LOG(at .. ' G ' .. state(g))
        for i = 1, 5 do LOG(at .. ' H' .. i .. ' ' .. state(group[i])) end
        LOG(at .. ' T ' .. state(t) .. pod(t, 'Front_Right_Engine'))
        LOG(at .. ' S ' .. state(s) .. pod(s, 'Jet_Front'))
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
