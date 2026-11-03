-- Scenario test (tests/scenarios.rs): the CZAR's Quantum Beam Generator is
-- fixed (Turreted = false) along its Center_Muzzle bone, which points
-- straight down: it fires only at a target within its FiringTolerance (30
-- degrees) of that line. Told to attack a tank off to the side, the CZAR
-- flies over it and only then fires: every shot within 30 degrees of down.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function make(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
local function run()
    WaitTicks(2)
    local czar = make('uaa0310', 'ARMY_1', 400, 600)
    WaitTicks(40)
    local dx, dy, dz = czar:GetBoneDirection('Center_Muzzle')
    note(string.format('muzzle direction %.2f %.2f %.2f, height %.1f', dx, dy, dz, czar:GetPosition()[2] - GetTerrainHeight(400, 600)))
    local tank = make('uel0201', 'ARMY_2', 424, 600)
    local shots, widest = 0, 0
    local beam = czar:GetWeaponByLabel('QuantumBeamGeneratorWeapon')
    local fire = beam.OnFire
    beam.OnFire = function(self, a, b, c)
        if not tank.Dead then
            local p, q = czar:GetPosition(), tank:GetPosition()
            local across = math.sqrt((p[1] - q[1]) ^ 2 + (p[3] - q[3]) ^ 2)
            widest = math.max(widest, math.deg(math.atan2(across, p[2] - q[2])))
            shots = shots + 1
        end
        return fire(self, a, b, c)
    end
    IssueAttack({ czar }, tank)
    WaitTicks(150)
    note(string.format('%d shots, the widest %.1f degrees off straight down', shots, widest))
    check(shots > 0, 'the CZAR got over the tank and fired')
    check(widest <= 32, 'every shot within the 30 degree firing tolerance of straight down')
    ScenarioResult.done = true
end
ForkThread(function()
    local ok, err = pcall(run)
    if not ok then check(false, tostring(err)) ScenarioResult.done = true end
end)
