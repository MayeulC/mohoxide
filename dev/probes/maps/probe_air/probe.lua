-- Differential probe: aircraft flight, logged per tick in a format both
-- engines print identically ("PROBE ..." lines). Engine APIs only, so it runs
-- on FAF's ForgedAlliance.exe as well as on our engine with FAF's Lua.
--   real game (Windows): dev/probes/run_faf.ps1 probe_air
--   ours: cargo run -- --headless --no-ai --ticks 460 --script dev/probes/maps/probe_air/probe.lua
-- A: a UEF T3 fighter flies 200 east from rest, turns round and flies back,
--    then idles (circling) where it stopped.
-- B, C: a UEF and a Cybran T3 fighter dogfight, ordered onto each other.
-- D: a UEF air factory's T1 scout, no rally point: how it leaves the pad
--    and where (and how high) it goes before landing.
-- E: a UEF T1 interceptor (MinAirspeed 10, StartTurnDistance 5) sent to a
--    point, then idle: whether it gets close enough to land or orbits.
local TICKS = 450

local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

local armies = ListArmies()
local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z) + 20, z, 0, heading, 0)
end

-- Quaternion {x, y, z, w} applied to v.
local function rotate(q, v)
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
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
        .. ' v=' .. f2(10 * math.sqrt(vx * vx + vy * vy + vz * vz)) .. ' vy=' .. f2(10 * vy)
        .. ' hp=' .. math.floor(u:GetHealth())
end

local a = spawn('uea0303', armies[1], cx - 100, cz - 150, math.pi / 2)
local b = spawn('uea0303', armies[1], cx - 40, cz + 150, math.pi / 2)
local c = spawn('ura0303', armies[2], cx + 40, cz + 150, -math.pi / 2)
local brain = GetArmyBrain(armies[1])
brain:GiveStorage('MASS', 50000) brain:GiveStorage('ENERGY', 500000)
brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
local fx, fz = cx - 150, cz + 300
local factory = CreateUnitHPR('ueb0102', armies[1], fx, GetTerrainHeight(fx, fz), fz, 0, 0, 0)
factory:SetBuildRate(400)
IssueBuildFactory({ factory }, 'uea0101', 1)
local d
local ex, ez = cx + 150, cz + 300
local e = spawn('uea0102', armies[1], ex - 80, ez, math.pi / 2)
LOG('PROBE armies ', table.getn(armies), ' map ', w, ' ', h, ' factory ', fx, ' ', fz)

ForkThread(function()
    local start = GetGameTick()
    for t = 0, TICKS do
        if t == 30 then
            IssueMove({ a }, { cx + 100, GetTerrainHeight(cx + 100, cz - 150), cz - 150 })
            IssueAttack({ b }, c)
            IssueAttack({ c }, b)
            IssueMove({ e }, { ex, GetTerrainHeight(ex, ez), ez })
        elseif t == 180 then
            IssueClearCommands({ a })
            IssueMove({ a }, { cx - 100, GetTerrainHeight(cx - 100, cz - 150), cz - 150 })
        end
        LOG('PROBE t=' .. (GetGameTick() - start) .. ' A ' .. state(a))
        LOG('PROBE t=' .. (GetGameTick() - start) .. ' B ' .. state(b))
        LOG('PROBE t=' .. (GetGameTick() - start) .. ' C ' .. state(c))
        if not d then
            for _, u in brain:GetListOfUnits(categories.uea0101, false) or {} do
                if u:GetFractionComplete() >= 1 then d = u end
            end
        end
        LOG('PROBE t=' .. (GetGameTick() - start) .. ' E ' .. state(e) .. ' layer=' .. e:GetCurrentLayer())
        if d then LOG('PROBE t=' .. (GetGameTick() - start) .. ' D ' .. state(d) .. ' layer=' .. d:GetCurrentLayer()) end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
