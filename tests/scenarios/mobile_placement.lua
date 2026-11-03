-- Scenario test (tests/scenarios.rs): where the cheat menu's spawn
-- ghost (CanBuildStructureAt / the placement query) allows a mobile unit.
-- Moho's OCCUPY_Check (MohoEngine.dll 0x10153ab0) only asks whether a mobile
-- blueprint's footprint can stand there (OCCUPY_MobileCheck): ships on water,
-- tanks on land, hovercraft and aircraft on both; structures keep their own
-- rules. Then the callback the spawn click sends puts the ship on the water.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
-- Seton's Clutch: deep water at (300, 300), flat land at (220, 700).
local water, land = { 300, 300 }, { 220, 700 }
-- { blueprint, allowed on water, allowed on land }
local cases = {
    { 'ues0103', true, false },  -- frigate
    { 'ues0302', true, false },  -- battleship (a big footprint)
    { 'ues0203', true, false },  -- submarine
    { 'uel0201', false, true },  -- tank
    { 'xsl0103', true, true },   -- hover tank
    { 'uea0102', true, true },   -- interceptor
    { 'ueb0103', true, false },  -- naval factory (a structure on water)
    { 'urb0101', false, true },  -- land factory
}
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    for _, c in cases do
        for i, p in { water, land } do
            local ok = b:CanBuildStructureAt(c[1], Vector(p[1], GetSurfaceHeight(p[1], p[2]), p[2]))
            check(ok == c[i + 1], string.format('%s on %s: %s, want %s', c[1], i == 1 and 'water' or 'land', tostring(ok), tostring(c[i + 1])))
        end
    end
    -- The spawn click's callback, at the ground the cursor is over (the seabed).
    local before = table.getn(b:GetListOfUnits(categories.ues0103, false))
    import('/lua/simcallbacks.lua').DoCallback('CheatSpawnUnit', { army = 1, pos = { water[1], GetTerrainHeight(water[1], water[2]), water[2] }, bpId = 'ues0103', count = 1, yaw = 0 }, {})
    WaitTicks(3)
    local ships = b:GetListOfUnits(categories.ues0103, false)
    check(table.getn(ships) == before + 1, 'CheatSpawnUnit made a frigate')
    local ship = ships[table.getn(ships)]
    if ship then
        local _, y = ship:GetPositionXYZ()
        check(ship:GetCurrentLayer() == 'Water' and math.abs(y - GetSurfaceHeight(water[1], water[2])) < 0.5,
            string.format('the frigate floats (layer %s, y %.2f)', ship:GetCurrentLayer(), y))
    end
    ScenarioResult.done = true
end)
