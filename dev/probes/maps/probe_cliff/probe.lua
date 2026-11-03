-- Differential probe: which cells each pathing class can stand on at cliffs
-- and shores, and whether units' steering keeps to them, logged
-- in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_cliff 240
--   ours: cargo run -- --headless --no-ai --ticks 1500 --script dev/probes/maps/probe_cliff/probe.lua
-- Moho's cell rule (`OCCUPY_MobileCheck`, MohoEngine.dll 0x101532e0, read
-- statically): water layers need the footprint's shallowest corner at least
-- `MinWaterDepth` under water, ground layers its deepest at most
-- `MaxWaterDepth` under and no corner edge steeper than `MaxSlope`.
-- 1. `CanPathTo` over two strips of cells, per class: Seton's shore cliff
--    west of the middle (192..214 x 600..632) and a beach in the north
--    (604..632 x 280..296), for uel0201 (Vehicle1x1), url0203
--    (Amphibious1x1), uel0105 (AmphibiousFloating, WaterLand1x1), uel0203
--    (Hover, WaterLand2x2) and ues0103 (Water4x4). One row of 0/1 per z.
-- 2. 36 tanks (one move), 36 url0203 and 36 uel0203 (formation moves) sent
--    off the cliff out to sea and back: unit-ticks on a cell the rule
--    forbids (the unit's own cell, so a necessary test for the 2x2 hover).
-- 3. The engineer builds a naval factory off the beach: where it stands,
--    whether the factory starts.
local armies = ListArmies()
local water = GetSurfaceHeight(300, 616)

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local function spawn(bp, x, z)
    return CreateUnitHPR(bp, armies[1], x, GetTerrainHeight(x, z), z, 0, 0, 0)
end
local function num(v) return v and string.format('%.2f', v) or '-' end

-- Corner heights of cell (x, z): max edge step, shallowest and deepest depth.
local function cell(x, z)
    local h00, h10 = GetTerrainHeight(x, z), GetTerrainHeight(x + 1, z)
    local h01, h11 = GetTerrainHeight(x, z + 1), GetTerrainHeight(x + 1, z + 1)
    local step = math.max(math.abs(h00 - h10), math.abs(h00 - h01), math.abs(h10 - h11), math.abs(h01 - h11))
    return step, water - math.max(h00, h10, h01, h11), water - math.min(h00, h10, h01, h11)
end

ForkThread(function()
    WaitTicks(2)
    sweep({})
    local brain = ArmyBrains[1]
    brain:GiveStorage('MASS', 50000) brain:GiveStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    LOG('PROBE info water ' .. num(water))

    -- 1. CanPathTo per class.
    local probes = {
        { tag = 'T', bp = 'uel0201', x = 185, z = 610 },
        { tag = 'A', bp = 'url0203', x = 185, z = 625 },
        { tag = 'E', bp = 'uel0105', x = 626, z = 288 },
        { tag = 'H', bp = 'uel0203', x = 175, z = 605 },
        { tag = 'F', bp = 'ues0103', x = 300, z = 616 },
    }
    for _, p in probes do
        p.u = spawn(p.bp, p.x, p.z)
        local fp = __blueprints[p.bp].Footprint or {}
        local x, y, z = p.u:GetPositionXYZ()
        LOG(string.format('PROBE info unit %s %s %.2f %.2f %.2f %s footprint %s %s caps %s slope %s min %s max %s', p.tag, p.bp, x, y, z,
            p.u:GetCurrentLayer(), tostring(fp.SizeX), tostring(fp.SizeZ), tostring(fp.OccupancyCaps or fp.Caps), num(fp.MaxSlope), num(fp.MinWaterDepth), num(fp.MaxWaterDepth)))
    end
    -- (The real game builds its path clusters in the background.)
    WaitTicks(30)
    local strips = { cliff = { 192, 214, 600, 632 }, beach = { 604, 632, 280, 296 } }
    for _, name in { 'cliff', 'beach' } do
        local s = strips[name]
        for z = s[3], s[4] do
            local terrain = ''
            for x = s[1], s[2] do
                local step, shallowest, deepest = cell(x, z)
                terrain = terrain .. string.format(' %.2f/%.2f/%.2f', step, shallowest, deepest)
            end
            LOG('PROBE cell ' .. name .. ' z=' .. z .. terrain)
            for _, p in probes do
                local row = ''
                for x = s[1], s[2] do
                    local ok = p.u:CanPathTo({ x + 0.5, GetSurfaceHeight(x + 0.5, z + 0.5), z + 0.5 })
                    row = row .. (ok and '1' or '0')
                end
                LOG('PROBE path ' .. p.tag .. ' ' .. name .. ' z=' .. z .. ' ' .. row)
            end
        end
    end
    for _, p in probes do p.u:Destroy() end
    WaitTicks(2)

    -- 2. Off the cliff and back; 3. the naval factory.
    local rules = { uel0201 = { max = 0.05 }, url0203 = { max = 25 }, uel0203 = { max = 1, min = 0.1 } }
    local function group(bp, x0, z0)
        local g = {}
        for i = 0, 35 do
            table.insert(g, spawn(bp, x0 - math.mod(i, 6) * 2.5, z0 + math.floor(i / 6) * 2.5))
        end
        return g
    end
    local tanks, amphibious, hover = group('uel0201', 185, 590), group('url0203', 185, 625), group('uel0203', 170, 605)
    local all = {}
    for _, g in { tanks, amphibious, hover } do
        for _, u in g do table.insert(all, u) end
    end
    local engineer = spawn('uel0105', 626, 288)
    IssueMove(tanks, { 300, 0, 616 })
    IssueFormMove(amphibious, { 300, 0, 616 }, 'AttackFormation', 0)
    IssueFormMove(hover, { 300, 0, 600 }, 'AttackFormation', 0)
    IssueBuildMobile({ engineer }, { 608.5, GetSurfaceHeight(608.5, 288.5), 288.5 }, 'ueb0103', {})
    local bad, first = { uel0201 = 0, url0203 = 0, uel0203 = 0 }, {}
    local factory
    for t = 1, 1300 do
        WaitTicks(1)
        if t == 600 then
            IssueClearCommands(all)
            IssueMove(tanks, { 150, 0, 600 })
            IssueFormMove(amphibious, { 150, 0, 630 }, 'AttackFormation', 0)
            IssueFormMove(hover, { 160, 0, 615 }, 'AttackFormation', 0)
        end
        for _, u in all do
            if not u:BeenDestroyed() then
                local x, _, z = u:GetPositionXYZ()
                local r = rules[u.UnitId]
                local step, shallowest, deepest = cell(math.floor(x), math.floor(z))
                if not ((step <= 0.75 and deepest <= r.max) or (r.min and shallowest >= r.min)) then
                    bad[u.UnitId] = bad[u.UnitId] + 1
                    if not first[u.UnitId] then
                        first[u.UnitId] = true
                        LOG(string.format('PROBE move first %s t=%d at %.2f %.2f step %.2f depth %.2f..%.2f', u.UnitId, t, x, z, step, shallowest, deepest))
                    end
                end
            end
        end
        if not factory then
            local f = brain:GetListOfUnits(categories.ueb0103, false) or {}
            if f[1] then
                factory = f[1]
                local fx, fy, fz = factory:GetPositionXYZ()
                local ex, _, ez = engineer:GetPositionXYZ()
                LOG(string.format('PROBE naval started t=%d factory %.2f %.2f %.2f engineer %.2f %.2f', t, fx, fy, fz, ex, ez))
            end
        end
        if math.mod(t, 50) == 0 then
            local ex, _, ez = engineer:GetPositionXYZ()
            local p1, p2, p3 = tanks[1]:GetPosition(), amphibious[1]:GetPosition(), hover[1]:GetPosition()
            LOG(string.format('PROBE move t=%d bad tank=%d amph=%d hover=%d | T %.1f %.1f A %.1f %.1f H %.1f %.1f | E %.2f %.2f %s', t,
                bad.uel0201, bad.url0203, bad.uel0203, p1[1], p1[3], p2[1], p2[3], p3[1], p3[3], ex, ez, engineer:GetCurrentLayer()))
        end
    end
    LOG('PROBE naval ' .. (factory and 'built' or 'never started'))
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
