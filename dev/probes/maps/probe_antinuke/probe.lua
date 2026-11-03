-- Differential probe: how a strategic missile defence's interceptor flies
-- at a nuke (does it lead the nuke or chase it round?). Logged in a format
-- both engines print identically.
--   real game (Windows): dev/probes/run_faf.ps1 probe_antinuke 60
--   ours: cargo run -- --headless --no-ai --ticks 600 --script dev/probes/maps/probe_antinuke/probe.lua
-- A UEF SMD (ueb4302) with 1 interceptor beside a T1 power generator (army
-- 1); a UEF nuke launcher (ueb2305, army 2) fires 1 nuke at the generator.
-- Every tick from launch: the game tick, each missile's position ('N' the
-- nuke, 'I' interceptors, numbered in launch order; 'dead' once gone);
-- then each missile's launch tick and the distance it flew, and the tick
-- the nuke was intercepted.
local armies = ListArmies()

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function make(bp, army, x, z)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    u:SetCanTakeDamage(false)
    return u
end

ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    local smd = make('ueb4302', armies[1], 420.5, 580.5)
    local site = make('ueb1101', armies[1], 430.5, 580.5)
    local silo = make('ueb2305', armies[2], 650.5, 190.5)
    local down = 0
    local intercepted = silo.OnMissileIntercepted
    silo.OnMissileIntercepted = function(self, a, b, c)
        down = down + 1
        LOG('PROBE ', GetGameTick(), ' intercepted')
        if intercepted then return intercepted(self, a, b, c) end
    end
    local shots = {}
    local function wrap(w, tag)
        local create = w.CreateProjectileAtMuzzle
        w.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            if p then table.insert(shots, { tag = tag, p = p, tick = GetGameTick(), dist = 0 }) end
            return p
        end
    end
    wrap(smd:GetWeapon(1), 'I')
    wrap(silo:GetWeapon(1), 'N')
    WaitTicks(5)
    smd:GiveTacticalSiloAmmo(1)
    silo:GiveNukeSiloAmmo(1)
    IssueNuke({silo}, site:GetPosition())
    local last = {}
    for _ = 1, 500 do
        WaitTicks(1)
        local s = 'PROBE ' .. GetGameTick()
        for i, sh in shots do
            if sh.p:BeenDestroyed() then
                s = s .. ' ' .. sh.tag .. i .. ' dead'
            else
                local x, y, z = sh.p:GetPositionXYZ()
                if last[i] then sh.dist = sh.dist + VDist3({x, y, z}, last[i]) end
                last[i] = {x, y, z}
                s = s .. string.format(' %s%d %.1f,%.1f,%.1f', sh.tag, i, x, y, z)
            end
        end
        LOG(s)
        if down > 0 then break end
    end
    for i, sh in shots do LOG(string.format('PROBE %s%d launched %d flew %.1f', sh.tag, i, sh.tick, sh.dist)) end
    LOG('PROBE done intercepted ', down)
end)
