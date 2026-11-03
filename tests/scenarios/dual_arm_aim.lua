-- Scenario test (tests/scenarios.rs): dual-arm weapons
-- (`TurretDualManipulators`). FAF aims them with a 'Torso' aim that only
-- yaws, over the blueprint's whole yaw range, and two arm aims that pitch
-- (the right one the fire control), their own yaw a twelfth of the
-- torso's. A Mech Marine and a Titan with a tank 90 degrees to their
-- right: the torso turns to it and they shoot it, and the right arm's
-- muzzle points at it (the arms don't turn again on top of the torso).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end

local CX, CZ = 400, 600

local function trial(id, muzzle)
    local shooter = CreateUnitHPR(id, 'ARMY_1', CX, GetTerrainHeight(CX, CZ), CZ, 0, 0, 0)
    local target = CreateUnitHPR('uel0201', 'ARMY_2', CX + 10, GetTerrainHeight(CX + 10, CZ), CZ, 0, 0, 0)
    target:SetFireState(1)
    shooter:SetFireState(0)
    target:SetMaxHealth(1e7)
    target:SetHealth(nil, 1e7)
    WaitTicks(40)
    -- The muzzle's heading off the line to the target (degrees).
    local x, _, z = shooter:GetBoneDirection(muzzle)
    local p, t = shooter:GetPosition(muzzle), target:GetPosition()
    local off = math.deg(math.atan2(x, z) - math.atan2(t[1] - p[1], t[3] - p[3]))
    if off > 180 then off = off - 360 elseif off < -180 then off = off + 360 end
    WaitTicks(40)
    local dealt = 1e7 - target:GetHealth()
    note(string.format('%s: dealt %d, %s heading %.1f deg off the target', id, dealt, muzzle, off))
    check(dealt > 0, id .. ' shoots a tank 90 degrees to its right')
    check(math.abs(off) < 20, id .. "'s right arm points at the tank")
    shooter:Destroy()
    target:Destroy()
    WaitTicks(5)
end

ForkThread(function()
    WaitTicks(2)
    local ok, e = pcall(function()
        trial('uel0106', 'Arm_Right_Muzzle')
        trial('uel0303', 'Muzzle_R')
    end)
    if not ok then check(false, tostring(e)) end
    ScenarioResult.done = true
end)
