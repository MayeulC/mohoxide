-- Scenario test (tests/scenarios.rs): a tank driving into a tree calls the
-- tree's OnCollision(unit, nx, ny, nz, depth) once (FAF fells it, away from
-- the tank); a tree group gets OnCollision(unit, vector) and breaks up.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local x, z = 400, 600
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
    local tree = CreatePropHPR('/env/redrocks/props/trees/carotenoid01_s2_prop.bp', x + 10, GetTerrainHeight(x + 10, z), z, 0, 0, 0)
    local group = CreatePropHPR('/env/tundra/props/trees/groups/tund_pine01_group1_prop.bp', x + 20, GetTerrainHeight(x + 20, z), z, 0, 0, 0)
    local hits, normal = 0, nil
    local orig = tree.OnCollision
    tree.OnCollision = function(self, other, nx, ny, nz, depth)
        hits = hits + 1
        normal = { nx, nz, other }
        return orig(self, other, nx, ny, nz, depth)
    end
    local broke = false
    local origGroup = group.OnCollision
    group.OnCollision = function(self, other, vec)
        broke = true
        return origGroup(self, other, vec)
    end
    WaitTicks(2)
    IssueMove({tank}, { x + 30, GetTerrainHeight(x + 30, z), z })
    for _ = 1, 150 do
        WaitTicks(1)
        if table.getn(tank:GetCommandQueue()) == 0 then break end
    end
    check(hits == 1, 'tree told once: ' .. hits)
    if normal then
        -- Ahead of the tank; it may clip it to one side (a tank's reach is
        -- its footprint, 1x1).
        check(normal[1] > 0 and normal[3] == tank, string.format('falls away from the tank (nx %.2f)', normal[1]))
    end
    check(tree.Fallen, 'FAF felled the tree')
    check(broke, 'tree group broke up')
    check(group:BeenDestroyed(), 'group replaced by its trees')
    ScenarioResult.done = true
end)
