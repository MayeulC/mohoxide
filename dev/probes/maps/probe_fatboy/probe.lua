-- Differential probe: where a mobile unit an engineer builds (the Fatboy,
-- uel0401, NEEDMOBILEBUILD) stands and how it's turned on sloping ground
--, logged in a format both engines print identically
-- ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_fatboy 150
--   ours: cargo run -- --headless --no-ai --ticks 700 --script dev/probes/maps/probe_fatboy/probe.lua
-- Spots on dry land are picked by the spread of the ground under the
-- Fatboy's size (centre and the four corners at +-SizeX/2, +-SizeZ/2), one
-- per spread band, two sets:
-- B. an engineer beside the spot is ordered to build it there
--    (`IssueBuildMobile`), with `CanBuildStructureAt` logged first; the
--    construction is logged when it starts (and 1 and 5 ticks on), part
--    built and once finished (and 1 and 10 ticks on);
-- C. made by `CreateUnitHPR` at the spot (logged at once and a few ticks on);
-- D. the ground under C4 flattened at tick 12: does a unit standing still
--    follow it?
local armies = ListArmies()

local function sweep()
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep()

local bp = 'uel0401'
local hx, hz = __blueprints[bp].SizeX / 2, __blueprints[bp].SizeZ / 2

local function ground(x, z)
    local c = GetTerrainHeight(x, z)
    local a, b = GetTerrainHeight(x - hx, z - hz), GetTerrainHeight(x + hx, z - hz)
    local d, e = GetTerrainHeight(x - hx, z + hz), GetTerrainHeight(x + hx, z + hz)
    return c, a, b, d, e
end

local function spread(x, z)
    local c, a, b, d, e = ground(x, z)
    return math.max(c, a, b, d, e) - math.min(c, a, b, d, e)
end

local function dry(x, z)
    for _, p in { { x, z }, { x - hx, z - hz }, { x + hx, z - hz }, { x - hx, z + hz }, { x + hx, z + hz } } do
        if GetSurfaceHeight(p[1], p[2]) > GetTerrainHeight(p[1], p[2]) then return false end
    end
    return true
end

local bands = { { 0.3, 0.8 }, { 0.8, 1.5 }, { 1.5, 2.5 }, { 2.5, 3.5 }, { 3.5, 5 }, { 5, 7 }, { 7, 10 }, { 10, 30 } }

-- Two spots per band, at least 40 apart from every other spot.
local function pick()
    local spots, found = {}, {}
    for i = 1, table.getn(bands) do found[i] = {} end
    for x = 80, 940, 6 do
        for z = 80, 940, 6 do
            local s = spread(x, z)
            for i, band in bands do
                if s >= band[1] and s < band[2] and table.getn(found[i]) < 2 and dry(x, z) then
                    local clear = true
                    for _, p in spots do
                        if math.abs(p.x - x) < 40 and math.abs(p.z - z) < 40 then clear = false end
                    end
                    if clear then
                        local p = { x = x, z = z, band = i, spread = s }
                        table.insert(spots, p)
                        table.insert(found[i], p)
                    end
                end
            end
        end
    end
    return found
end

local function describe(tag, u)
    local x, y, z = u:GetPositionXYZ()
    local o = u:GetOrientation()
    local c, a, b, d, e = ground(x, z)
    LOG(string.format('PROBE %s pos %.3f %.3f %.3f orient %.4f %.4f %.4f %.4f ground c %.3f -- %.3f +- %.3f -+ %.3f ++ %.3f frac %.2f',
        tag, x, y, z, o[1], o[2], o[3], o[4], c, a, b, d, e, u:GetFractionComplete()))
end

ForkThread(function()
    WaitTicks(2)
    sweep()
    local brain = ArmyBrains[1]
    brain:GiveStorage('MASS', 500000) brain:GiveStorage('ENERGY', 5000000)
    brain:GiveResource('MASS', 500000) brain:GiveResource('ENERGY', 5000000)
    LOG(string.format('PROBE info size %.2f %.2f', hx * 2, hz * 2))
    local found = pick()

    -- B. Ordered built.
    local sites = {}
    for i, list in found do
        local s = list[1]
        if s then
            local y = GetTerrainHeight(s.x, s.z)
            local ok = brain:CanBuildStructureAt(bp, { s.x, y, s.z })
            LOG(string.format('PROBE B%d site %d %d spread %.3f can %s', i, s.x, s.z, s.spread, tostring(ok)))
            local e = CreateUnitHPR('uel0309', armies[1], s.x + 8, GetTerrainHeight(s.x + 8, s.z), s.z, 0, 0, 0)
            e:SetBuildRate(4000)
            IssueBuildMobile({ e }, { s.x, y, s.z }, bp, {})
            table.insert(sites, { tag = 'B' .. i, e = e })
        end
    end

    -- C. Made outright.
    local made = {}
    for i, list in found do
        local s = list[2]
        if s then
            local u = CreateUnitHPR(bp, armies[1], s.x, GetTerrainHeight(s.x, s.z), s.z, 0, 0, 0)
            LOG(string.format('PROBE C%d site %d %d spread %.3f', i, s.x, s.z, s.spread))
            describe('C' .. i .. ' t0', u)
            table.insert(made, { tag = 'C' .. i, u = u })
        end
    end

    for t = 1, 400 do
        WaitTicks(1)
        if t == 1 or t == 10 or t == 13 or t == 20 then
            for _, m in made do describe(m.tag .. ' t' .. t, m.u) end
        end
        -- D. Does a finished unit standing still follow the ground when it
        -- changes under it? Flatten under C4 at tick 12.
        if t == 12 and made[4] then
            local x, _, z = made[4].u:GetPositionXYZ()
            FlattenMapRect(x - 6, z - 6, 12, 12, 22)
            LOG('PROBE D flattened under C4 to 22')
        end
        -- Topped up every tick (the stored amount runs dry otherwise).
        brain:GiveResource('MASS', 20000) brain:GiveResource('ENERGY', 200000)
        if math.mod(t, 25) == 0 and sites[4] and sites[4].u and not sites[4].u:BeenDestroyed() then
            LOG(string.format('PROBE progress t=%d B4 frac %.3f rate %.1f mass %.0f energy %.0f', t, sites[4].u:GetFractionComplete(),
                sites[4].e:GetBuildRate(), brain:GetEconomyStored('MASS'), brain:GetEconomyStored('ENERGY')))
        end
        local left = 0
        for _, s in sites do
            if not s.u then
                local u = s.e:GetFocusUnit()
                if u and u.UnitId == bp then
                    s.u, s.t = u, t
                    describe(s.tag .. ' start', u)
                end
            end
            -- Held up every tick: FAF's scripts put the rate back.
            if s.u and not s.done then s.e:SetBuildRate(4000) end
            if s.u and not s.u:BeenDestroyed() and (t == s.t + 1 or t == s.t + 5) then
                describe(s.tag .. ' start+' .. (t - s.t), s.u)
            end
            if s.u and not s.u:BeenDestroyed() then
                if not s.half and s.u:GetFractionComplete() >= 0.5 then
                    s.half = true
                    describe(s.tag .. ' half', s.u)
                end
                if not s.done and s.u:GetFractionComplete() >= 1 then
                    s.done = t
                    describe(s.tag .. ' done', s.u)
                end
                if s.done and t == s.done + 1 then
                    describe(s.tag .. ' done+1', s.u)
                end
                if s.done and t == s.done + 10 then
                    s.after = true
                    describe(s.tag .. ' after', s.u)
                end
            end
            if not s.after then left = left + 1 end
        end
        if left == 0 then break end
    end
    for _, s in sites do
        if not s.u then LOG(string.format('PROBE %s never started', s.tag)) end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
