-- Scenario test (tests/scenarios.rs): overcharge orders, against FAF
-- probe probe_auditgaps. IssueOverCharge fires the ACU's OverCharge weapon
-- once at the target, its damage scaled by stored energy and the energy
-- drained (OverchargeProjectile: CreateEconomyEvent + WaitFor), and the
-- order ends about 10 ticks after the shot. With less than EnergyRequired
-- stored no shot leaves and the order still ends.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function make(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
local fmt = string.format
SetAlliance('ARMY_1', 'ARMY_2', 'Enemy')
SetAlliance('ARMY_2', 'ARMY_1', 'Enemy')

-- An ACU and a 30000 hp tank 15 in front of it; counts OverCharge shots
-- and Overcharge damage.
local function lane(x)
    local acu = make('ual0001', 'ARMY_1', x, 600)
    local tank = make('uel0201', 'ARMY_2', x, 615)
    tank:SetFireState(1)
    tank:SetMaxHealth(30000) tank:SetHealth(nil, 30000)
    local l = { acu = acu, tank = tank, shots = {}, oc = 0 }
    local wep = acu:GetWeaponByLabel('OverCharge')
    local create = wep.CreateProjectileAtMuzzle
    wep.CreateProjectileAtMuzzle = function(self, muzzle)
        table.insert(l.shots, GetGameTick())
        return create(self, muzzle)
    end
    local on = tank.OnDamage
    tank.OnDamage = function(self, inst, amount, vec, kind)
        if kind == 'Overcharge' then l.oc = l.oc + amount end
        return on(self, inst, amount, vec, kind)
    end
    return l
end
local function queued(u) return table.getn(u:GetCommandQueue() or {}) end

ForkThread(function()
    WaitTicks(2)
    local brain = GetArmyBrain('ARMY_1')
    brain:__engine_SetStorage('ENERGY', 60000)
    brain:GiveResource('ENERGY', 60000)
    local full = lane(400)
    WaitTicks(3)
    local before = brain:GetEconomyStored('ENERGY')
    check(before > 50000, fmt('energy stored for the shot: %.0f', before))
    local issued = GetGameTick()
    IssueOverCharge({ full.acu }, full.tank)
    local emptied
    for _ = 1, 40 do
        WaitTicks(2)
        if not emptied and queued(full.acu) == 0 then emptied = GetGameTick() end
    end
    check(table.getn(full.shots) == 1, fmt('one OverCharge shot: %d', table.getn(full.shots)))
    local shot = full.shots[1]
    check(shot and shot - issued <= 4, fmt('fired soon after the order: tick %s, ordered %d', tostring(shot), issued))
    check(shot and emptied and emptied - shot >= 8 and emptied - shot <= 12, fmt('the order ends ~10 ticks after the shot: shot %s, empty %s', tostring(shot), tostring(emptied)))
    -- 0.9 of the stored energy / 6 (FAF's lua/shared/overcharge.lua).
    local want = before * 0.9 / 6
    check(math.abs(full.oc - want) < want * 0.05, fmt('damage scaled by stored energy: %.1f, about %.1f', full.oc, want))
    local after = brain:GetEconomyStored('ENERGY')
    check(after < before - full.oc * 5, fmt('energy drained (6 per damage): %.0f -> %.0f', before, after))
    full.acu:Destroy() full.tank:Destroy()

    -- Too little energy: no shot, the order ends anyway.
    brain:TakeResource('ENERGY', brain:GetEconomyStored('ENERGY'))
    brain:GiveResource('ENERGY', 3000)
    local low = lane(360)
    WaitTicks(3)
    check(brain:GetEconomyStored('ENERGY') < 7500, 'below EnergyRequired')
    IssueOverCharge({ low.acu }, low.tank)
    local start = GetGameTick()
    emptied = nil
    for _ = 1, 20 do
        WaitTicks(2)
        if not emptied and queued(low.acu) == 0 then emptied = GetGameTick() end
    end
    check(table.getn(low.shots) == 0, fmt('no shot without the energy: %d', table.getn(low.shots)))
    check(low.oc == 0, 'no Overcharge damage')
    check(emptied and emptied - start <= 16, fmt('the order still ends: issued %d, empty %s', start, tostring(emptied)))
    ScenarioResult.done = true
end)
