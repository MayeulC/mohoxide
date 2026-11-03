-- Scenario test (tests/scenarios.rs): collision detectors. A walking ACU's
-- footfall detector (FAF's CreateFootFallManipulators) reports each foot
-- landing (OnAnimCollision(bone, x, y, z)), alternating feet; a detector
-- with EnableTerrainCheck reports OnAnimTerrainCollision instead.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local acu = CreateUnitHPR('uel0001', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local bones = {}
    local orig = acu.OnAnimCollision
    acu.OnAnimCollision = function(self, bone, x, y, z)
        table.insert(bones, bone)
        return orig(self, bone, x, y, z)
    end
    WaitTicks(5)
    check(table.getn(bones) == 0, 'no footfalls standing still')
    IssueMove({acu}, {400, GetTerrainHeight(400, 640), 640})
    WaitTicks(40)
    check(table.getn(bones) >= 4, 'footfalls while walking: ' .. table.getn(bones))
    check(bones[1] ~= bones[2] and bones[1] == bones[3], 'alternating feet: ' .. table.concat(bones, ' '))

    local terrain = 0
    local walker = CreateUnitHPR('uel0001', 'ARMY_1', 420, GetTerrainHeight(420, 600), 600, 0, 0, 0)
    local d = CreateCollisionDetector(walker)
    d:WatchBone('Left_Footfall')
    d:EnableTerrainCheck(true)
    d:Enable()
    walker.OnAnimTerrainCollision = function(self, bone, x, y, z) terrain = terrain + 1 end
    IssueMove({walker}, {420, GetTerrainHeight(420, 640), 640})
    WaitTicks(40)
    check(terrain >= 2, 'terrain check reports OnAnimTerrainCollision: ' .. terrain)
    ScenarioResult.done = true
end)
