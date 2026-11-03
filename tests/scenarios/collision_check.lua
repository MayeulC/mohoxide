-- Scenario test (tests/scenarios.rs): Unit:OnCollisionCheck vetoes hits.
-- FAF refuses collisions for units with DisallowCollisions (dying,
-- upgrading): shots pass through until it's cleared.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function make(bp, army, x, z, h) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, h or 0, 0) end
ForkThread(function()
    WaitTicks(2)
    local target = make('uel0201', 'ARMY_1', 402.5, 600.5)
    target:SetFireState(1) -- hold fire
    target.DisallowCollisions = true
    local tank = make('uel0201', 'ARMY_2', 402.5, 612.5, 3.14159)
    IssueAttack({tank}, target)
    WaitTicks(60)
    check(target:GetHealth() >= target:GetMaxHealth(), 'shots pass through a unit that refuses collisions')
    target.DisallowCollisions = false
    WaitTicks(60)
    check(target:GetHealth() < target:GetMaxHealth(), 'and hit once it accepts them')
    ScenarioResult.done = true
end)
