-- Differential probe: where a structure ends up when it's made, or ordered
-- built, at a point off the cell grid, logged in a format both engines print
-- identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_snap 120
--   ours: cargo run -- --headless --no-ai --ticks 400 --script dev/probes/maps/probe_snap/probe.lua
-- Structures of different footprint and skirt sizes (odd and even), each at
-- four offsets from a whole cell:
-- 1. made by `CreateUnitHPR` (and one turned a quarter);
-- 2. ordered by `IssueBuildMobile` from an engineer beside the site.
local armies = ListArmies()

local function sweep()
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep()

local function spawn(bp, x, z, heading)
    return CreateUnitHPR(bp, armies[1], x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end

local bps = { 'ueb2101', 'ueb1101', 'ueb1201', 'ueb0101', 'ueb0103' }
local offsets = { { 0, 0 }, { 0.5, 0.5 }, { 0.25, 0.75 }, { 0.5, 0 } }

ForkThread(function()
    WaitTicks(2)
    sweep()
    local brain = ArmyBrains[1]
    brain:GiveStorage('MASS', 50000) brain:GiveStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    for _, bp in bps do
        local fp = __blueprints[bp].Footprint or {}
        local ph = __blueprints[bp].Physics or {}
        LOG(string.format('PROBE info %s footprint %s %s skirt %s %s offset %s %s', bp, tostring(fp.SizeX), tostring(fp.SizeZ),
            tostring(ph.SkirtSizeX), tostring(ph.SkirtSizeZ), tostring(ph.SkirtOffsetX), tostring(ph.SkirtOffsetZ)))
    end

    -- 1. Made at the point.
    for i = 1, 4 do
        local bp = bps[i]
        for j, o in offsets do
            local x, z = 600 + (j - 1) * 14 + o[1], 300 + (i - 1) * 14 + o[2]
            local u = spawn(bp, x, z)
            local ux, uy, uz = u:GetPositionXYZ()
            LOG(string.format('PROBE create %s at %.2f %.2f -> %.2f %.2f (y %.3f, ground %.3f)', bp, x, z, ux, uz, uy, GetTerrainHeight(ux, uz)))
        end
    end
    local turned = spawn('ueb0101', 670.25, 300.75, math.pi / 2)
    local tx, _, tz = turned:GetPositionXYZ()
    LOG(string.format('PROBE create turned ueb0101 at 670.25 300.75 -> %.2f %.2f', tx, tz))

    -- 2. Ordered built at the point: whole numbers both odd and even (a
    -- half of the footprint start rounds to even), and the naval factory
    -- (4 x 10) off the beach.
    local builds = {
        { 'ueb2101', 600, 380 }, { 'ueb2101', 615, 381 }, { 'ueb2101', 629.5, 381.5 }, { 'ueb2101', 643.25, 382.75 },
        { 'ueb1101', 600, 394 }, { 'ueb1101', 615, 395 }, { 'ueb1101', 628.25, 394.75 }, { 'ueb1101', 642.5, 394 },
        { 'ueb0101', 600, 422 }, { 'ueb0101', 615, 423 }, { 'ueb0101', 628.25, 422.75 }, { 'ueb0101', 642.5, 422 },
        { 'ueb0103', 610.5, 291.5, 626, 288 },
    }
    local sites = {}
    for _, b in builds do
        local bp, x, z = b[1], b[2], b[3]
        local e = spawn('uel0105', b[4] or x + 6, b[5] or z)
        IssueBuildMobile({ e }, { x, GetSurfaceHeight(x, z), z }, bp, {})
        table.insert(sites, { bp = bp, x = x, z = z, e = e })
    end
    for t = 1, 300 do
        WaitTicks(1)
        local left = 0
        for _, s in sites do
            if not s.done then
                local u = s.e:GetFocusUnit()
                if u and u.UnitId == s.bp then
                    s.done = true
                    local ux, _, uz = u:GetPositionXYZ()
                    LOG(string.format('PROBE build %s at %.2f %.2f -> %.2f %.2f t=%d', s.bp, s.x, s.z, ux, uz, t))
                else
                    left = left + 1
                end
            end
        end
        if left == 0 then break end
    end
    for _, s in sites do
        if not s.done then LOG(string.format('PROBE build %s at %.2f %.2f never started', s.bp, s.x, s.z)) end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
