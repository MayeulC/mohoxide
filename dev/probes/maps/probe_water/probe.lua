-- Differential probe: units and torpedoes at the water surface, logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_water 40
--   ours: cargo run -- --headless --no-ai --ticks 200 --script dev/probes/maps/probe_water/probe.lua
-- A (349): army 1 subs / a frigate created on the seabed, and a sub at the
-- surface, idle on hold fire: their height over the water and layer.
-- B (350): an army 1 sub at the surface fires its torpedoes at an army 2 sub
-- that has dived: each torpedo's launch height and what it hits, and (353)
-- its position and velocity (per tick) every tick of its flight.
local armies = ListArmies()

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local ids = {}
local function spawn(bp, army, x, y, z)
    local u = CreateUnitHPR(bp, army, x, y, z, 0, 0, 0)
    ids[u:GetEntityId()] = true
    u:SetFireState(1)
    return u
end
local function f(v) return string.format('%.2f', v) end

-- A: deep water around z 60 (depth ~13).
local a = {}
for i, s in { { 'ues0203', 'seabed' }, { 'ues0103', 'seabed' }, { 'ues0203', 'surface' } } do
    local x, z = 120 + i * 40, 60
    local y = s[2] == 'seabed' and GetTerrainHeight(x, z) or GetSurfaceHeight(x, z)
    table.insert(a, { name = s[1] .. '@' .. s[2], u = spawn(s[1], armies[1], x, y, z), x = x, z = z })
end

-- B: z 140, 25 apart.
local shooter = spawn('ues0203', armies[1], 200, GetSurfaceHeight(200, 140), 140)
local target = spawn('ues0203', armies[2], 225, GetSurfaceHeight(225, 140), 140)
target:SetMaxHealth(1e7) target:SetHealth(nil, 1e7)
IssueDive({ target })
local w = shooter:GetWeaponByLabel('Torpedo01')
local fired = 0
local torpedoes = {}
local start = 0
if w then
    local create = w.CreateProjectileAtMuzzle
    w.CreateProjectileAtMuzzle = function(self, muzzle)
        local p = create(self, muzzle)
        fired = fired + 1
        local n = fired
        if p then
            local q = p:GetPosition()
            local x, z = q[1], q[3]
            LOG('PROBE torpedo ' .. n .. ' launched at ' .. (GetGameTick() - start) .. ' over the water ' .. f(q[2] - GetSurfaceHeight(q[1], q[3])))
            torpedoes[n] = p
            local impact = p.OnImpact
            p.OnImpact = function(pr, kind, other)
                local r = pr:GetPosition()
                LOG('PROBE torpedo ' .. n .. ' impact ' .. tostring(kind) .. ' at ' .. (GetGameTick() - start) .. ' after ' .. f(VDist2(x, z, r[1], r[3])))
                return impact(pr, kind, other)
            end
        end
        return p
    end
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    start = GetGameTick()
    while GetGameTick() - start <= 150 do
        local t = GetGameTick() - start
        if t <= 2 or math.mod(t, 10) == 0 then
            local row = 'PROBE ' .. t
            for _, c in a do
                local p = c.u:GetPosition()
                row = row .. ' ' .. c.name .. '=' .. f(p[2] - GetSurfaceHeight(p[1], p[3])) .. '/' .. c.u:GetCurrentLayer()
            end
            local tp = target:GetPosition()
            row = row .. ' target=' .. f(tp[2] - GetSurfaceHeight(tp[1], tp[3])) .. '/' .. target:GetCurrentLayer()
            LOG(row)
        end
        -- 353: each torpedo's flight, every tick.
        for n, p in torpedoes do
            if p:BeenDestroyed() then
                torpedoes[n] = nil
            else
                local q, vx, vy, vz = p:GetPosition(), p:GetVelocity()
                LOG('PROBE flight ' .. t .. ' torpedo ' .. n .. ' at ' .. f(q[1]) .. ',' .. f(q[2] - GetSurfaceHeight(q[1], q[3])) .. ',' .. f(q[3])
                    .. ' velocity ' .. f(vx) .. ',' .. f(vy) .. ',' .. f(vz))
            end
        end
        if t == 60 then shooter:SetFireState(0) end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
