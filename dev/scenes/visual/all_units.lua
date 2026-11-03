-- Every unit in the game on one map, for looking at the graphics: grouped by
-- blueprint prefix (UEL, UAB, URS, XSA, civilians...), naval units on water,
-- everything else on land, all on hold fire so nothing fights.
-- cargo run -- --no-ai --script dev/scenes/visual/all_units.lua --camera 120,120,60
-- Set ALL_UNITS_FILTER (a Lua pattern on the blueprint id, e.g. '^xs') before
-- this script runs to place only some.
ForkThread(function()
    WaitTicks(2)
    local filter = rawget(_G, 'ALL_UNITS_FILTER')

    local ids = {}
    for id, bp in __blueprints do
        if type(id) == 'string' and bp.Categories and bp.Display and bp.Display.Mesh
            and id == string.lower(bp.BlueprintId or id)
            and not string.find(id, '/')
            and not (bp.CategoriesHash and (bp.CategoriesHash.DUMMYUNIT or bp.CategoriesHash.INSIGNIFICANTUNIT))
            and (not filter or string.find(id, filter))
        then
            table.insert(ids, id)
        end
    end
    table.sort(ids)

    local size = 1024
    if GetMapSize then size = GetMapSize() end
    local margin, gap = 30, 4
    local function water_at(x, z) return GetSurfaceHeight(x, z) > GetTerrainHeight(x, z) + 1.5 end
    local function naval(bp)
        local mt = bp.Physics and bp.Physics.MotionType
        return mt == 'RULEUMT_Water' or mt == 'RULEUMT_SurfacingSub'
    end
    local function cell(bp)
        local f = bp.Footprint or {}
        local p = bp.Physics or {}
        return math.max(f.SizeX or 1, f.SizeZ or 1, p.SkirtSizeX or 0, p.SkirtSizeZ or 0, bp.SizeX or 1, bp.SizeZ or 1) + gap
    end

    -- Two cursors scan the map in rows: one for land spots, one for water.
    local cursors = {
        land = { x = margin, z = margin, row = 0, prefix = nil },
        water = { x = margin, z = margin, row = 0, prefix = nil },
    }
    local function next_row(c)
        c.x = margin
        c.z = c.z + c.row + gap
        c.row = 0
    end
    local function place(c, w, wants_water, prefix)
        -- A new prefix starts a new row so groups read as rows.
        if c.prefix and c.prefix ~= prefix and c.x > margin then next_row(c) end
        c.prefix = prefix
        for _ = 1, 4000 do
            if c.x + w > size - margin then next_row(c) end
            if c.z + w > size - margin then return nil end
            local x, z = c.x + w / 2, c.z + w / 2
            local ok = water_at(x, z) == wants_water and water_at(c.x, c.z) == wants_water and water_at(c.x + w, c.z + w) == wants_water
            c.x = c.x + w
            if ok then
                c.row = math.max(c.row, w)
                return x, z
            end
        end
    end

    local placed, skipped = 0, 0
    for _, id in ids do
        local bp = __blueprints[id]
        local wants_water = naval(bp)
        local c = wants_water and cursors.water or cursors.land
        local x, z = place(c, cell(bp), wants_water, string.sub(id, 1, 3))
        if x then
            local ok, unit = pcall(CreateUnitHPR, id, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
            if ok and unit then
                if unit.SetFireState then unit:SetFireState(1) end
                placed = placed + 1
                LOG('ALL_UNITS ', id, ' at ', math.floor(x), ',', math.floor(z))
            else
                skipped = skipped + 1
                LOG('ALL_UNITS could not create ', id, ': ', tostring(unit))
            end
        else
            skipped = skipped + 1
            LOG('ALL_UNITS no room for ', id)
        end
    end
    LOG('ALL_UNITS placed ', placed, ' units, skipped ', skipped)
end)
