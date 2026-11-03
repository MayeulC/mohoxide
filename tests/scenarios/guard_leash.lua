-- Scenario test (tests/scenarios.rs): a Striker on an aggressive move east
-- chases an enemy engineer walking north (within its GuardScanRadius) to
-- stay in weapon range, breaks off once it's GuardReturnRadius from where
-- the chase began, and carries on east. The radius is cut to 25 so it all
-- fits on the plateau.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
ForkThread(function()
    WaitTicks(2)
    __blueprints['uel0201'].AI.GuardReturnRadius = 25
    local striker = CreateUnitHPR('uel0201', 'ARMY_1', 380, ground(380, 600), 600, 0, 0, 0)
    local eng = CreateUnitHPR('uel0105', 'ARMY_2', 380, ground(380, 586), 586, 0, 0, 0)
    -- Eyes well past the chase: a moving unit's sight trails it (redrawn
    -- once it's moved a third of its radius), and with its own 20 it loses
    -- the engineer a few steps in. The leash is what's tested here.
    striker:SetIntelRadius('Vision', 60)
    -- It soaks the fire for the whole walk.
    eng:SetMaxHealth(1e7) eng:SetHealth(eng, 1e7)
    -- Its weapon on the engineer first.
    WaitTicks(30)
    IssueMove({eng}, {380, 0, 520})
    IssueAggressiveMove({striker}, {440, 0, 600})
    local minZ = 600
    for _ = 1, 90 do
        WaitTicks(5)
        minZ = math.min(minZ, striker:GetPosition()[3])
    end
    local p = striker:GetPosition()
    note(string.format('furthest north z %.1f, end %.1f, %.1f', minZ, p[1], p[3]))
    check(minZ < 588, 'chased the engineer north (furthest z ' .. minZ .. ')')
    check(minZ > 600 - 25 - 4, 'broke off near GuardReturnRadius 25 from where it began (furthest z ' .. minZ .. ')')
    check(p[1] > 425, 'then carried on east (x ' .. p[1] .. ')')
    done()
end)
