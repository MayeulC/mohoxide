-- Scenario test (tests/scenarios.rs): `UseFiringSolutionInsteadOfAimBone`.
-- A Hunter (its gun has the flag) given a 180 degree firing tolerance and a
-- turret that barely turns, a tank standing 90 degrees to its right: it
-- fires at once, its barrel still facing ahead. With the flag the shot
-- leaves along the firing solution, at the tank; without it (the flag
-- cleared on a second Hunter), along the barrel.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end

-- Land on the scenario map.
local CX, CZ = 400, 600

-- The first shot's launch heading off the target (degrees), or nil if none.
local function first_shot(flag)
    local w = __blueprints['url0106'].Weapon[1]
    w.UseFiringSolutionInsteadOfAimBone = flag
    w.FiringTolerance = 180
    w.TurretYawSpeed = 1
    local shooter = CreateUnitHPR('url0106', 'ARMY_1', CX, GetTerrainHeight(CX, CZ), CZ, 0, 0, 0)
    local target = CreateUnitHPR('uel0201', 'ARMY_2', CX + 10, GetTerrainHeight(CX + 10, CZ), CZ, 0, 0, 0)
    target:SetFireState(1)
    shooter:SetFireState(0)
    target:SetMaxHealth(1e7)
    target:SetHealth(nil, 1e7)
    local err
    local weapon = shooter:GetWeapon(1)
    local orig = weapon.CreateProjectileForWeapon
    weapon.CreateProjectileForWeapon = function(self, ...)
        local proj = orig(self, ...)
        if proj and not err then
            local vx, _, vz = proj:GetVelocity()
            local p, t = proj:GetPosition(), target:GetPosition()
            err = math.deg(math.atan2(vx, vz) - math.atan2(t[1] - p[1], t[3] - p[3]))
            if err > 180 then err = err - 360 elseif err < -180 then err = err + 360 end
        end
        return proj
    end
    for i = 1, 60 do
        WaitTicks(1)
        if i % 20 == 0 then note('dbg '..i..' target '..tostring(weapon:GetCurrentTarget())..' wstate '..tostring(weapon.WeaponState or (weapon.GetCurrentState and 'x'))) end
        if err then break end
    end
    shooter:Destroy()
    target:Destroy()
    WaitTicks(5)
    return err
end

ForkThread(function()
    WaitTicks(2)
    local ok, e = pcall(function()
    local with = first_shot(true)
    local without = first_shot(false)
    note(string.format('first shot heading off the target: with the flag %s deg, without %s deg', tostring(with), tostring(without)))
    check(with and math.abs(with) < 10, 'with UseFiringSolutionInsteadOfAimBone the shot leaves at the target')
    check(without and math.abs(without) > 60, 'without it the shot leaves along the barrel, still facing ahead')
    end)
    if not ok then check(false, tostring(e)) end
    ScenarioResult.done = true
end)
