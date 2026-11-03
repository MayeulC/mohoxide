-- Differential probe: how often air weapons lose and gain targets in a
-- dogfight, and why they lose them, logged in a format both engines print.
--   real game (Windows): dev/probes/run_faf.ps1 probe_targets 120
--   ours: cargo run -- --headless --no-ai --ticks 340 --script dev/probes/maps/probe_targets/probe.lua
-- 60 v 60 aircraft (the 10k air duel's: UEF interceptors, fighter-bombers,
-- ASF against Aeon's), 120 apart, attack-moving onto each other at t=0.
-- Each tick every weapon's target is polled. Each OnLostTarget (FAF's
-- unit hook, called by every weapon's OnLostTarget) is classed by the
-- target the weapon held at the last poll: 'dead' (destroyed or dying),
-- 'far' (beyond MaxRadius x TrackingRadius, flat distance), 'near' (still
-- within it: dropped for its arc or another check). 'got' counts a weapon's
-- polled target changing to a new unit. Every 10 ticks: totals so far.
local TICKS = 300
local armies = ListArmies()
local FACTIONS = { { 'uea0102', 'dea0202', 'uea0303' }, { 'uaa0102', 'xaa0202', 'uaa0303' } }

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local ids, units, weapons = {}, { {}, {} }, {}
local count = { got = 0, lost = 0, dead = 0, far = 0, near = 0 }
local last = {}

local function gone(u)
    return (not u) or u:BeenDestroyed() or u.Dead
end

local function onLost(self, weapon)
    count.lost = count.lost + 1
    local held = last[weapon]
    if not held or gone(held.target) then
        count.dead = count.dead + 1
        return
    end
    local s, t = self:GetPosition(), held.target:GetPosition()
    local dx, dz = s[1] - t[1], s[3] - t[3]
    if dx * dx + dz * dz > held.reach * held.reach then
        count.far = count.far + 1
    else
        count.near = count.near + 1
    end
end

for a = 1, 2 do
    local cx, cz = 300 + (a - 1) * 120, 600
    for k = 0, 59 do
        local x, z = cx + math.mod(k, 8) * 4, cz + math.floor(k / 8) * 4
        local u = CreateUnitHPR(FACTIONS[a][math.mod(k, 3) + 1], armies[a], x, GetTerrainHeight(x, z) + 20, z, 0, 0, 0)
        ids[u:GetEntityId()] = true
        table.insert(units[a], u)
    end
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    for a = 1, 2 do
        for _, u in units[a] do
            u.OnLostTarget = onLost
            for i = 1, u:GetWeaponCount() do
                local w = u:GetWeapon(i)
                local bp = w:GetBlueprint()
                table.insert(weapons, { weapon = w, unit = u, reach = (bp.MaxRadius or 0) * (bp.TrackingRadius or 1) })
            end
        end
        -- Onto the other side's block.
        IssueAggressiveMove(units[a], { a == 1 and 434 or 314, 0, 614 })
    end
    local start = GetGameTick()
    LOG('PROBE info start=' .. start .. ' weapons=' .. table.getn(weapons))
    while GetGameTick() - start <= TICKS do
        local t = GetGameTick() - start
        for _, e in weapons do
            if not gone(e.unit) then
                local target = e.weapon:GetCurrentTarget()
                local prev = last[e.weapon]
                if target and not (prev and rawequal(prev.target, target)) then
                    count.got = count.got + 1
                end
                last[e.weapon] = target and { target = target, reach = e.reach } or nil
            end
        end
        if math.mod(t, 10) == 0 then
            local alive = { 0, 0 }
            for a = 1, 2 do for _, u in units[a] do if not gone(u) then alive[a] = alive[a] + 1 end end end
            LOG('PROBE ' .. t .. ' alive=' .. alive[1] .. '/' .. alive[2] .. ' got=' .. count.got .. ' lost=' .. count.lost .. ' dead=' .. count.dead .. ' far=' .. count.far .. ' near=' .. count.near)
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
