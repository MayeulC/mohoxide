-- Scenario test (tests/scenarios.rs): unit:CanPathTo(goal), which FAF's AI
-- asks before sending land and naval units (aiutilities.lua CanPathToCurrentEnemy).
-- One boolean back, as moho's binding.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local other = CreateUnitHPR('uel0201', 'ARMY_1', 430, GetTerrainHeight(430, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local ok, second = tank:CanPathTo({420, GetTerrainHeight(420, 610), 610})
    check(ok == true, 'a tank reaches open ground nearby')
    check(second == nil, 'one result')
    check(tank:CanPathTo(other) == true, 'and another unit on it')
    -- Deep water: a tank can't drive there.
    local wx, wz
    for x = 32, 992, 16 do
        for z = 32, 992, 16 do
            if GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) > 8 then wx, wz = x, z break end
        end
        if wx then break end
    end
    if wx then
        check(tank:CanPathTo({wx, GetSurfaceHeight(wx, wz), wz}) == false, 'a tank can\'t reach deep water')
    else
        table.insert(ScenarioResult.notes, 'no deep water on this map: water check skipped')
    end
    ScenarioResult.done = true
end)
