-- Scenario test (tests/scenarios.rs): attack-ground and `CannotAttackGround`.
-- A mobile AA unit (uel0104, AAGun CannotAttackGround) and a tank (uel0201)
-- are both told to attack a patch of ground in reach: the tank shells it,
-- the AA gun holds fire (it only ever shoots what it picks itself: aircraft).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function make(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
ForkThread(function()
    WaitTicks(2)
    local aa = make('uel0104', 400, 600)
    local tank = make('uel0201', 410, 600)
    local shots = { aa = 0, tank = 0 }
    -- (uel0104's weapon 1 is a zero-damage helper; its gun is `AAGun`.)
    for name, w in { aa = aa:GetWeaponByLabel('AAGun'), tank = tank:GetWeapon(1) } do
        local fire = w.OnFire
        w.OnFire = function(self, a, b, c)
            shots[name] = shots[name] + 1
            return fire(self, a, b, c)
        end
    end
    local spot = { 405, GetTerrainHeight(405, 612), 612 }
    IssueAttack({ aa }, spot)
    IssueAttack({ tank }, spot)
    WaitTicks(150)
    note(string.format('shots at the ground: AA %d, tank %d', shots.aa, shots.tank))
    check(shots.tank > 0, 'the tank fired at the ground')
    check(shots.aa == 0, 'the AA gun did not')
    ScenarioResult.done = true
end)
