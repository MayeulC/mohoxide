-- Differential probe: air-to-air fights, logged per tick in a format both
-- engines print identically ("PROBE ..." lines). Engine APIs only (Lua 5.0
-- safe), so it runs on FAF's ForgedAlliance.exe as well as ours.
--   real game (Windows): dev/probes/run_faf.ps1 probe_dogfight 240
--   ours: cargo run -- --headless --no-ai --ticks 620 --script dev/probes/maps/probe_dogfight/probe.lua
-- Fights, each in its own area, ordered onto each other at tick 20:
--   duel 1-3: a UEF T3 fighter against a Cybran one, 100 apart;
--   duel 4-5: a UEF T1 interceptor against a Cybran one, 80 apart;
--   brawl: five UEF T3 fighters against five Cybran, ordered onto the
--   nearest enemy.
-- Every plane: id tag, position, height, heading / pitch / roll, speed, hp.
local TICKS = 600

local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

local armies = ListArmies()

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
        .. ' hp=' .. math.floor(u:GetHealth())
end

local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, armies[army], x, GetTerrainHeight(x, z) + 20, z, 0, heading, 0)
end

local planes, fights = {}, {}
local function fight(tag, a, b)
    table.insert(fights, { a = a, b = b })
    table.insert(planes, { tag = tag .. 'a', u = a })
    table.insert(planes, { tag = tag .. 'b', u = b })
end
local e = math.pi / 2
fight('d1', spawn('uea0303', 1, 200, 200, e), spawn('ura0303', 2, 300, 200, -e))
fight('d2', spawn('uea0303', 1, 200, 400, e), spawn('ura0303', 2, 300, 400, -e))
fight('d3', spawn('uea0303', 1, 600, 200, e), spawn('ura0303', 2, 700, 200, -e))
fight('d4', spawn('uea0102', 1, 600, 400, e), spawn('ura0102', 2, 680, 400, -e))
fight('d5', spawn('uea0102', 1, 200, 800, e), spawn('ura0102', 2, 280, 800, -e))
local blue, red = {}, {}
for i = 1, 5 do
    local a = spawn('uea0303', 1, 560, 700 + i * 12, e)
    local b = spawn('ura0303', 2, 700, 700 + i * 12, -e)
    table.insert(blue, a) table.insert(red, b)
    table.insert(planes, { tag = 'bb' .. i, u = a })
    table.insert(planes, { tag = 'br' .. i, u = b })
end
LOG('PROBE planes ' .. table.getn(planes))

ForkThread(function()
    local start = GetGameTick()
    for t = 0, TICKS do
        if t == 20 then
            for _, f in fights do
                IssueAttack({ f.a }, f.b)
                IssueAttack({ f.b }, f.a)
            end
            for i = 1, 5 do
                IssueAttack({ blue[i] }, red[i])
                IssueAttack({ red[i] }, blue[i])
            end
        end
        local at = 'PROBE t=' .. (GetGameTick() - start) .. ' '
        for _, p in planes do LOG(at .. p.tag .. ' ' .. state(p.u)) end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
