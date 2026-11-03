-- Scenario test (tests/scenarios.rs): entity:GetCollisionExtents, as FAF's
-- props cache it (Prop.CacheAndRemoveCollisionExtents / Apply...) and its
-- objective arrows size themselves by it.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local x, z = 400, 600
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
    WaitTicks(2)
    local ok, ext = pcall(tank.GetCollisionExtents, tank)
    check(ok, 'GetCollisionExtents runs: ' .. tostring(ext))
    if not ok then WARN('collision extents scenario: ' .. tostring(ext)) ScenarioResult.done = true return end
    local p = tank:GetPosition()
    check(ext.Min.x < p[1] and ext.Max.x > p[1] and ext.Min.z < p[3] and ext.Max.z > p[3], 'the box is around the tank')
    check(ext.Max.y > p[2], 'and above its base')
    local size = tank.Blueprint.SizeX or 1
    check(math.abs((ext.Max.x - ext.Min.x) - size) < 0.6, string.format('about its size: %.2f vs %.2f', ext.Max.x - ext.Min.x, size))
    -- A prop's cached extents restore the same box.
    local rock = CreatePropHPR('/env/crystalline/props/rocks/cryssmrock01_prop.bp', x + 10, GetTerrainHeight(x + 10, z), z, 0, 0, 0)
    WaitTicks(1)
    local before = rock:GetCollisionExtents()
    rock.CachePosition = rock:GetPosition()
    rock:CacheAndRemoveCollisionExtents()
    rock:ApplyCachedCollisionExtents()
    local after = rock:GetCollisionExtents()
    check(math.abs(before.Min.x - after.Min.x) < 0.01 and math.abs(before.Max.y - after.Max.y) < 0.01,
        string.format('cache round trip: %.2f,%.2f vs %.2f,%.2f', before.Min.x, before.Max.y, after.Min.x, after.Max.y))
    ScenarioResult.done = true
end)
