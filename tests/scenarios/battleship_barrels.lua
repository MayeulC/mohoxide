-- Scenario test (tests/scenarios.rs): rack bones slaved to the turret's pitch
-- bone (RackSlavedToTurret -> CreateSlaver, weapon.lua). The UEF battleship's
-- turrets pitch their middle barrel (TurretBonePitch) and slave the outer two
-- to it: aiming at a far target, all three barrels point the same way.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function wet(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) > 3 end
ForkThread(function()
    WaitTicks(2)
    -- Deep water along +Z for the ship and its target.
    local wx, wz
    for x = 128, 896, 8 do
        for z = 128, 780, 8 do
            local ok = true
            for d = 0, 50, 10 do
                if not (wet(x, z + d) and wet(x - 12, z + d) and wet(x + 12, z + d)) then ok = false break end
            end
            if ok then wx, wz = x, z break end
        end
        if wx then break end
    end
    if not wx then
        table.insert(ScenarioResult.notes, 'no open water on this map: skipped')
        ScenarioResult.done = true
        return
    end
    local ship = CreateUnitHPR('ues0302', 'ARMY_1', wx, GetSurfaceHeight(wx, wz), wz, 0, 0, 0)
    local target = CreateUnitHPR('ues0103', 'ARMY_2', wx, GetSurfaceHeight(wx, wz + 40), wz + 40, 0, 0, 0)
    target:SetCanTakeDamage(false)
    target:SetFireState(1)
    IssueAttack({ ship }, target)
    WaitTicks(120)
    local function dir(bone)
        local x, y, z = ship:GetBoneDirection(bone)
        return { x, y, z }
    end
    for _, turret in { 'Front_Turret01', 'Front_Turret02', 'Back_Turret' } do
        local mid = dir(turret .. '_Barrel02')
        check(math.abs(mid[2]) > 0.02, string.format('%s pitched to aim: %.3f %.3f %.3f', turret, mid[1], mid[2], mid[3]))
        for _, b in { '_Barrel01', '_Barrel03' } do
            local d = dir(turret .. b)
            local dot = d[1] * mid[1] + d[2] * mid[2] + d[3] * mid[3]
            check(dot > 0.9999, string.format('%s%s follows the middle barrel: %.3f %.3f %.3f vs %.3f %.3f %.3f',
                turret, b, d[1], d[2], d[3], mid[1], mid[2], mid[3]))
        end
    end
    ScenarioResult.done = true
end)
