-- Scenario test (tests/scenarios.rs): death debris (Unit.lua
-- CreateUnitDestructionDebris) ties a part's bone to a debris projectile
-- with AttachBoneToEntityBone.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local radar = CreateUnitHPR('urb3103', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    WaitTicks(2)
    local links = 0
    local attach = radar.AttachBoneToEntityBone
    radar.AttachBoneToEntityBone = function(self, bone, other, otherBone, flag)
        links = links + 1
        return attach(self, bone, other, otherBone, flag)
    end
    local ok, err = pcall(radar.CreateUnitDestructionDebris, radar, true, false, false)
    check(ok, 'CreateUnitDestructionDebris runs: ' .. tostring(err))
    if not ok then WARN('debris toss scenario: ' .. tostring(err)) end
    check(links == 1, 'the spinner follows a debris projectile: ' .. links)
    WaitTicks(5)
    ScenarioResult.done = true
end)
