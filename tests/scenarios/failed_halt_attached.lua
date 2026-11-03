-- Scenario test (tests/scenarios.rs): OnFailedToBeBuilt on a factory build
-- that's cancelled (FAF destroys it), parent:OnAttachedKilled(unit) when an
-- attached unit dies, and a weapon's OnHaltFire when its attack order's
-- ground target moves.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    -- Factory build cancelled half way.
    local factory = CreateUnitHPR('ueb0101', 'ARMY_1', 360, GetTerrainHeight(360, 560), 560, 0, 0, 0)
    WaitTicks(2)
    IssueBuildFactory({factory}, 'uel0201', 1)
    local built
    for _ = 1, 60 do
        WaitTicks(1)
        built = factory:GetFocusUnit()
        if built and built:GetFractionComplete() > 0 then break end
    end
    check(built ~= nil, 'factory started building')
    if built then
        local failed = false
        local orig = built.OnFailedToBeBuilt
        built.OnFailedToBeBuilt = function(self) failed = true; return orig(self) end
        IssueClearCommands({factory})
        WaitTicks(4)
        check(failed, 'OnFailedToBeBuilt on the cancelled build')
        check(built:BeenDestroyed(), 'FAF destroyed the cancelled build')
    end

    -- Attached unit killed.
    local carrier = CreateUnitHPR('uel0201', 'ARMY_1', 420, GetTerrainHeight(420, 560), 560, 0, 0, 0)
    local cargo = CreateUnitHPR('uel0201', 'ARMY_1', 420, GetTerrainHeight(420, 560), 560, 0, 0, 0)
    WaitTicks(2)
    local reported
    carrier.OnAttachedKilled = function(self, attached) reported = attached end
    cargo:AttachBoneTo(-1, carrier, -1)
    WaitTicks(1)
    cargo:Kill()
    WaitTicks(1)
    check(reported == cargo, 'carrier told its cargo died')

    -- Attack order re-pointed at a new spot.
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 640), 640, 0, 0, 0)
    WaitTicks(2)
    local w = tank:GetWeapon(1)
    local halts = 0
    w.OnHaltFire = function(self) halts = halts + 1 end
    IssueAttack({tank}, {408, GetTerrainHeight(408, 640), 640})
    WaitTicks(10)
    check(halts == 0, 'no halt while the target holds')
    IssueClearCommands({tank})
    IssueAttack({tank}, {408, GetTerrainHeight(408, 646), 646})
    WaitTicks(3)
    check(halts == 1, 'one OnHaltFire when the target moved: ' .. halts)
    ScenarioResult.done = true
end)
