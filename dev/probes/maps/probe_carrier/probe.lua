-- Differential probe: a submerged Atlantis surfacing on its own,
-- logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_carrier 150
--   ours: cargo run -- --headless --no-ai --ticks 1200 --script dev/probes/maps/probe_carrier/probe.lua
-- The Atlantis dives; three interceptors are ordered aboard (does it surface,
-- when does each go in, in which layer); it dives again and is ordered to
-- unload 40 away (does it surface before it moves); it dives once more,
-- empty, and is ordered to unload (does the order end at once).
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local function f(v) return string.format('%.2f', v) end
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
-- Layer, the vertical unit states, cargo and queue length: logged when they
-- change, with where the carrier is.
local function state()
    local s = carrier:GetCurrentLayer()
    for _, n in { 'Diving', 'Surfacing', 'MovingUp', 'MovingDown' } do
        if carrier:IsUnitState(n) then s = s .. '+' .. n end
    end
    return s .. ' cargo ' .. table.getn(carrier:GetCargo()) .. ' queue ' .. table.getn(carrier:GetCommandQueue())
end
local function at()
    local p = carrier:GetPosition()
    return ' at ' .. f(p[1] - sx) .. ',' .. f(p[2] - GetSurfaceHeight(p[1], p[3])) .. ',' .. f(p[3] - sz)
end
-- Logs the carrier each tick its state changes, and every 20th, for `ticks`,
-- or until `stop` says so.
local function watch(phase, ticks, stop)
    local last
    for i = 0, ticks do
        local s = state()
        if s ~= last or math.mod(i, 20) == 0 then LOG('PROBE ' .. phase .. ' ' .. i .. ' ' .. s .. at()) last = s end
        if stop and stop() then LOG('PROBE ' .. phase .. ' stop ' .. i .. ' ' .. s .. at()) return end
        WaitTicks(1)
    end
end
local function dive(phase)
    if carrier:GetCurrentLayer() ~= 'Sub' then
        IssueDive({ carrier })
        watch(phase .. '-dive', 200, function() return carrier:GetCurrentLayer() == 'Sub' end)
    end
end

ForkThread(function()
    WaitTicks(5)
    start = GetGameTick()
    dive('load')
    local planes = {}
    for i = 1, 3 do
        local p = CreateUnitHPR('uea0102', armies[1], sx + 10 + i * 3, GetSurfaceHeight(sx, sz) + 15, sz + 6, 0, 0, 0)
        local n = i
        local add = p.OnAddToStorage
        p.OnAddToStorage = function(self, c)
            LOG('PROBE load plane ' .. n .. ' stored ' .. c:GetCurrentLayer())
            return add(self, c)
        end
        table.insert(planes, p)
    end
    WaitTicks(5)
    IssueTransportLoad(planes, carrier)
    watch('load', 300, function() return table.getn(carrier:GetCargo()) == 3 end)

    dive('unload')
    IssueTransportUnload({ carrier }, { sx + 40, GetSurfaceHeight(sx + 40, sz), sz })
    watch('unload', 300, function() return table.getn(carrier:GetCargo()) == 0 and table.getn(carrier:GetCommandQueue()) == 0 end)

    -- The planes away, so the carrier is empty.
    for _, p in planes do if not p:BeenDestroyed() then p:Destroy() end end
    WaitTicks(2)
    dive('empty')
    IssueTransportUnload({ carrier }, { sx, GetSurfaceHeight(sx, sz), sz })
    watch('empty', 30)
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
