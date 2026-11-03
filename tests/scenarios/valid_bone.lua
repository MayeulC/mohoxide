-- Scenario test (tests/scenarios.rs): Entity:IsValidBone as moho's binding
-- (MohoEngine.dll 0x1026ebd0): -1 is valid only with allowNil, nil is false
-- with allowNil and raises without, numbers are truncated.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function done() ScenarioResult.done = true end
ForkThread(function()
    WaitTicks(2)
    local u = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    local n = u:GetBoneCount()
    check(u:IsValidBone(0), 'bone 0')
    check(u:IsValidBone(n - 1), 'last bone')
    check(not u:IsValidBone(n), 'one past the last bone')
    check(u:IsValidBone(0.9), 'numbers are truncated')
    check(not u:IsValidBone(-1), '-1 without allowNil')
    check(u:IsValidBone(-1, true), '-1 with allowNil')
    check(u:IsValidBone(u:GetBoneName(0)), 'a bone name')
    check(not u:IsValidBone('NoSuchBone'), 'an unknown name')
    check(u:IsValidBone(nil, true) == false, 'nil with allowNil is false')
    check(not pcall(u.IsValidBone, u, nil), 'nil without allowNil raises')
    check(not pcall(u.IsValidBone, u, {}, true), 'a table raises')
    done()
end)
