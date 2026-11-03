-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
-- Flak bursts at the aircraft, not short of it. FAF sets a
-- `DetonatesAtTargetHeight` shell to burst as it climbs past the height of
-- `GetCurrentTargetPos`, which moho reads as the target's target point (its
-- centre here, MohoEngine.dll 0x102b5090 -> 0x101cf6c0), the point the
-- weapon aims at. Ours returned the unit's origin, 0.45 under a T1
-- transport's centre: the climbing shells burst 2-8 short, and the census
-- counted most flak a miss.
ForkThread(function()
    WaitTicks(2)
    local x, z = 400.5, 600.5
    local flak = CreateUnitHPR('uel0205', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
    local tz = z + 28
    local ty = GetSurfaceHeight(x, tz) + 8
    local target = CreateUnitHPR('uea0107', 'ARMY_2', x, ty, tz, 0, 0, 0)
    target:SetMaxHealth(1e6)
    target:SetHealth(nil, 1e6)
    -- An idle transport lands after 2 s: a move to its own spot every
    -- second keeps it up.
    ForkThread(function()
        while not ScenarioResult.done do
            IssueClearCommands({ target })
            IssueMove({ target }, { x, ty, tz })
            WaitTicks(10)
        end
    end)
    -- (Seen: the flak's own radar doesn't cover it.)
    import('/lua/sim/VizMarker.lua').VizMarker({ X = x, Z = tz, Radius = 15, LifeTime = -1, Army = GetArmyBrain('ARMY_1'):GetArmyIndex(),
        Omni = false, Radar = false, Vision = true, WaterVision = true })
    local w = flak:GetWeaponByLabel('AAGun')
    local bursts = {}
    local create = w.CreateProjectileAtMuzzle
    w.CreateProjectileAtMuzzle = function(self, muzzle)
        local p = create(self, muzzle)
        if p then
            local onImpact = p.OnImpact
            p.OnImpact = function(proj, kind, other)
                local at = proj:GetPosition()
                local t = target:GetPosition()
                table.insert(bursts, VDist2(at[1], at[3], t[1], t[3]))
                return onImpact(proj, kind, other)
            end
        end
        return p
    end
    IssueAttack({ flak }, target)
    local reported
    for _ = 1, 200 do
        WaitTicks(1)
        local pos = w:GetCurrentTargetPos()
        if pos and not reported then
            reported = pos[2] - target:GetPosition()[2]
        end
        if table.getn(bursts) >= 8 then break end
    end
    check(reported and reported > 0.3, 'GetCurrentTargetPos is the target point (its centre), above its origin: ' .. tostring(reported))
    local sum = 0
    for _, d in bursts do sum = sum + d end
    local mean = sum / math.max(1, table.getn(bursts))
    check(table.getn(bursts) >= 8, 'the flak fired: ' .. table.getn(bursts) .. ' bursts')
    check(mean < 2.5, string.format('bursts at the aircraft: %.2f from it on average', mean))
    ScenarioResult.done = true
end)
