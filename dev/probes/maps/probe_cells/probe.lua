-- Differential probe: which cells a vision disc covers, logged in
-- a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_cells 40
--   ours: cargo run -- --headless --no-ai --ticks 60 --script dev/probes/maps/probe_cells/probe.lua
-- Intel grids are 4-unit cells. An engineer (uel0105, VisionRadius 18) of
-- army 1 at a cell centre, an enemy engineer at an offset of (dx, dz) cells,
-- one pair per spot on Seton's flat area A, 50 apart. (4, 1), (3, 3), (4, 2)
-- are within 18 of the centre but further than 4 whole cells: moho's raster
-- (radius / 4 cells, rounded down) leaves them out. (4, 0), (2, 3) are in
-- either way, (5, 0) out. Every 10 ticks: whether army 1 sees each target.
local armies = ListArmies()

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local ids, cases = {}, {}
local function spawn(x, z, army)
    local u = CreateUnitHPR('uel0105', army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    ids[u:GetEntityId()] = true
    return u
end
local offsets = { { 4, 0 }, { 5, 0 }, { 4, 1 }, { 3, 3 }, { 4, 2 }, { 2, 3 } }
for i, o in offsets do
    -- Cell centres: 4c + 2.
    local x, z = 650 + math.mod(i - 1, 3) * 52, 102 + math.floor((i - 1) / 3) * 52
    local obs = spawn(x, z, armies[1])
    local tgt = spawn(x + o[1] * 4, z + o[2] * 4, armies[2])
    table.insert(cases, { name = o[1] .. ',' .. o[2], obs = obs, tgt = tgt })
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    local start = GetGameTick()
    while GetGameTick() - start <= 40 do
        local t = GetGameTick() - start
        if math.mod(t, 10) == 0 then
            local row = 'PROBE ' .. t
            for _, c in cases do
                local ok, r = pcall(function() local b = c.tgt:GetBlip(1) return b and b:IsSeenNow(1) end)
                row = row .. ' ' .. c.name .. '=' .. (not ok and '?' or (r and 'S' or '.'))
            end
            LOG(row)
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
