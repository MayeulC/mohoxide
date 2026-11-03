-- Scenario test (tests/scenarios.rs): MinBounceCount / MaxBounceCount make
-- projectiles bounce off the ground, BounceVelDamp scaling their velocity.
-- The UEF nuke's shockwave (Nuke/Shockwave01: 10000 bounces, damp 1) is
-- fired down at the ground and comes back up at the same speed; a riot
-- shell (ShellRiotTerran01: 1 or 2 bounces, damp default 0.5) comes back
-- up at half its speed.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function speed(p) local x, y, z = p:GetVelocity() return 10 * math.sqrt(x * x + y * y + z * z), 10 * y end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 570), 570, 0, 0, 0)
    local wave = tank:CreateProjectile('/effects/Nuke/Shockwave01_proj.bp', 0, 2, 0, 0, -1, 1)
    local shell = tank:CreateProjectile('/projectiles/ShellRiotTerran01/ShellRiotTerran01_proj.bp', 3, 2, 0, 0, -1, 0)
    shell:SetVelocity(0, -10, 0)
    shell:SetBallisticAcceleration(0)
    local before = speed(shell)
    WaitTicks(5)
    check(not wave:BeenDestroyed(), 'the shockwave is still there')
    check(not shell:BeenDestroyed(), 'the riot shell is still there')
    if not wave:BeenDestroyed() and not shell:BeenDestroyed() then
        local ws, wy = speed(wave)
        local ss, sy = speed(shell)
        local p = wave:GetPosition()
        note(string.format('shockwave %.2f (up %.2f) at %.2f above ground; shell %.2f -> %.2f (up %.2f)', ws, wy, p[2] - GetTerrainHeight(p[1], p[3]), before, ss, sy))
        check(wy > 0 and math.abs(ws - 10) < 0.1, 'the shockwave comes back up at its speed')
        check(p[2] > GetTerrainHeight(p[1], p[3]), 'above the ground')
        check(sy > 0 and math.abs(ss - before / 2) < 0.1, 'the riot shell comes back up at half its speed')
    end
    done()
end)
