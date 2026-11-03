-- Scenario test (tests/scenarios.rs): RotationalVelocity spins projectiles.
-- Unit-death debris (DebrisMisc01: 360 +- 180 degrees a second, not
-- velocity-aligned) tumbles; a velocity-aligned Chronatron shell
-- (SDFChronatronCannon01: 900) only rolls, its nose staying on its
-- velocity; SetLocalAngularVelocity(0, 0, 0) stops a spin (FAF's sinker).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
-- Degrees between two orientations ({x, y, z, w}).
local function turned(a, b)
    local d = math.abs(a[1] * b[1] + a[2] * b[2] + a[3] * b[3] + a[4] * b[4])
    return math.deg(2 * math.acos(math.min(d, 1)))
end
-- The +z axis of an orientation.
local function forward(q)
    local x, y, z, w = q[1], q[2], q[3], q[4]
    return 2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)
end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 570), 570, 0, 0, 0)
    local debris = tank:CreateProjectile('/effects/Entities/DebrisMisc01/DebrisMisc01_proj.bp', 0, 4, 0, 0, 1, 0)
    local stopped = tank:CreateProjectile('/effects/Entities/DebrisMisc01/DebrisMisc01_proj.bp', 2, 4, 0, 0, 1, 0)
    stopped:SetLocalAngularVelocity(0, 0, 0)
    local shell = tank:CreateProjectile('/projectiles/SDFChronatronCannon01/SDFChronatronCannon01_proj.bp', -2, 4, 0, 0, 0, 1)
    shell:SetVelocity(20)
    local d0, s0, h0 = debris:GetOrientation(), stopped:GetOrientation(), shell:GetOrientation()
    WaitTicks(2)
    local tumble, still, roll = turned(d0, debris:GetOrientation()), turned(s0, stopped:GetOrientation()), turned(h0, shell:GetOrientation())
    local fx, fy, fz = forward(shell:GetOrientation())
    local vx, vy, vz = shell:GetVelocity()
    local len = math.sqrt(vx * vx + vy * vy + vz * vz)
    local nose = math.deg(math.acos(math.min(1, (fx * vx + fy * vy + fz * vz) / len)))
    note(string.format('debris turned %.1f, stopped %.1f, shell rolled %.1f with its nose %.1f off its velocity', tumble, still, roll, nose))
    check(tumble > 10, 'debris tumbles (turned ' .. tumble .. ')')
    check(still < 0.01, 'SetLocalAngularVelocity(0, 0, 0) stops it (turned ' .. still .. ')')
    check(roll > 10, 'the aligned shell spins (turned ' .. roll .. ')')
    check(nose < 2, 'about its velocity (nose ' .. nose .. ' off)')
    done()
end)
