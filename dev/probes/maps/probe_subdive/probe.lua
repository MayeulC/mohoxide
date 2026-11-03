-- Differential probe: every RULEUMT_SurfacingSub diving and surfacing
--, logged in a format both engines print identically
-- ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_subdive 90
--   ours: cargo run -- --headless --no-ai --ticks 800 --script dev/probes/maps/probe_subdive/probe.lua
-- Two waves of the same units, each at its own deep-water spot: 'srf' created
-- at the water surface, 'bed' created on the seabed (as the weapon audit
-- spawns a submerged attacker). Each is left 20 ticks, then IssueDive; once
-- the layer has changed and the vertical states are clear for 5 ticks,
-- IssueDive again; done once that settles too (or 300 ticks per order).
-- Lines: 'PROBE <tick> <tag> made layer L y Y' (Y height relative to the
-- water surface), then every tick its height changes or its layer / states
-- change 'PROBE <tick> <tag> y Y L[+MovingUp|+MovingDown]', and
-- 'PROBE <tick> <tag> dive <n>' / 'settled <n> after <ticks>'.
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local fmt = string.format
local TESTER = armies[1]
local SUBS = {
    'ues0203', 'uas0203', 'urs0203', 'xss0203', 'xss0201', 'xas0204', 'xrs0204',
    'ues0304', 'uas0304', 'urs0304', 'xss0304', 'ues0401', 'uas0401', 'xsb3202',
}
-- Deep water on SCMP_009 (deeper than 12 within 20; probe_auditgaps), two
-- units per spot 20 apart.
local WATER = {
    { 48, 48 }, { 128, 48 }, { 208, 48 },
    { 208, 128 }, { 448, 320 }, { 176, 320 }, { 768, 656 }, { 624, 608 },
}
local function rel(u)
    local p = u:GetPosition()
    return p[2] - GetSurfaceHeight(p[1], p[3])
end
local function state(u)
    local s = tostring(u:GetCurrentLayer())
    for _, n in { 'MovingUp', 'MovingDown' } do
        if u:IsUnitState(n) then s = s .. '+' .. n end
    end
    return s
end

local function wave(name)
    local list = {}
    for i, bp in SUBS do
        local site = WATER[math.floor((i - 1) / 2) + 1]
        local x = site[1] + (math.mod(i - 1, 2) == 0 and -10 or 10)
        local z = site[2]
        local y = name == 'srf' and GetSurfaceHeight(x, z) or GetTerrainHeight(x, z)
        local ok, u = pcall(CreateUnitHPR, bp, TESTER, x, y, z, 0, 0, 0)
        if ok and u then
            u:SetFireState(1)
            local tag = name .. ':' .. bp
            LOG(fmt('PROBE %d %s made layer %s y %.4f seabed %.2f', GetGameTick(), tag, state(u), rel(u), GetTerrainHeight(x, z) - GetSurfaceHeight(x, z)))
            table.insert(list, { u = u, tag = tag, y = rel(u), s = state(u), phase = 0, t = 0, quiet = 0 })
        else
            LOG(fmt('PROBE %d %s:%s create failed %s', GetGameTick(), name, bp, tostring(u)))
        end
    end
    local busy = true
    local tick = 0
    while busy do
        WaitTicks(1)
        tick = tick + 1
        busy = false
        for _, w in list do
            if w.phase < 3 and not w.u:BeenDestroyed() then
                busy = true
                local y, s = rel(w.u), state(w.u)
                if math.abs(y - w.y) > 0.00005 or s ~= w.s then
                    LOG(fmt('PROBE %d %s y %.4f %s', GetGameTick(), w.tag, y, s))
                    w.quiet = 0
                else
                    w.quiet = w.quiet + 1
                end
                w.y, w.s = y, s
                w.t = w.t + 1
                if w.phase == 0 then
                    if tick >= 20 then
                        w.phase, w.t, w.layer0 = 1, 0, w.u:GetCurrentLayer()
                        IssueDive({ w.u })
                        LOG(fmt('PROBE %d %s dive 1', GetGameTick(), w.tag))
                    end
                else
                    local moving = w.u:IsUnitState('MovingUp') or w.u:IsUnitState('MovingDown')
                    local changed = w.u:GetCurrentLayer() ~= w.layer0
                    if (changed and not moving and w.quiet >= 5) or w.t >= 300 then
                        LOG(fmt('PROBE %d %s settled %d after %d layer %s y %.4f', GetGameTick(), w.tag, w.phase, w.t - w.quiet, tostring(w.u:GetCurrentLayer()), y))
                        if w.phase == 1 then
                            w.phase, w.t, w.layer0 = 2, 0, w.u:GetCurrentLayer()
                            IssueDive({ w.u })
                            LOG(fmt('PROBE %d %s dive 2', GetGameTick(), w.tag))
                        else
                            w.phase = 3
                        end
                    end
                end
            end
        end
    end
    for _, w in list do
        if not w.u:BeenDestroyed() then w.u:Destroy() end
    end
end

ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    wave('srf')
    WaitTicks(5)
    wave('bed')
    LOG(fmt('PROBE %d end', GetGameTick()))
end)
