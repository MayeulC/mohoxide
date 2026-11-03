-- Differential probe: does a projectile-targeting weapon (tactical
-- missile defence, which searches every tick) fire in the beat it takes its
-- target, or one beat later, as unit-targeting weapons do?
--   real game (Windows): dev/probes/run_faf.ps1 probe_tmdfire 60
--   ours: cargo run -- --headless --no-ai --ticks 400 --script dev/probes/maps/probe_tmdfire/probe.lua
-- One lane per faction, 100 apart: a T1 power generator (army 1) with the
-- faction's TMD 6 beside it, and the faction's TML (army 2) 200 away firing
-- one missile at the generator. Logged: the TMD's weapon callbacks
-- (OnGotTarget, OnLostTarget, OnFire) with the tick, and when the missile is
-- launched and gone.
-- Lines: PROBE <tick> <faction> <event>
local armies = ListArmies()

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function make(bp, army, x, z)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
end
local function P(f, s)
    LOG('PROBE ' .. GetGameTick() .. ' ' .. f .. ' ' .. s)
end

ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    local lanes = {}
    for i, fac in {'ue', 'ua', 'ur', 'xs'} do
        -- A plain local: moho's Lua 5.0 closures saw the loop variable as nil.
        local f = fac
        local z = 550 + i * 100
        local lane = { f = f }
        lane.site = make(f .. 'b1101', armies[1], 100.5, z)
        lane.tmd = make(f .. 'b4201', armies[1], 106.5, z)
        lane.tmd:SetCanTakeDamage(false)
        for k = 1, lane.tmd:GetWeaponCount() do
            local wep = lane.tmd:GetWeapon(k)
            local label = wep:GetBlueprint().Label
            local onFire, got, lost = wep.OnFire, wep.OnGotTarget, wep.OnLostTarget
            wep.OnFire = function(self, a, b)
                P(f, 'tmd onfire ' .. label)
                return onFire(self, a, b)
            end
            wep.OnGotTarget = function(self, a, b)
                P(f, 'tmd ongottarget ' .. label)
                return got(self, a, b)
            end
            wep.OnLostTarget = function(self, a, b)
                P(f, 'tmd onlosttarget ' .. label)
                return lost(self, a, b)
            end
        end
        lane.tml = make(f .. 'b2108', armies[2], 300.5, z)
        local w = lane.tml:GetWeapon(1)
        local create = w.CreateProjectileAtMuzzle
        w.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            if p and not lane.p then
                lane.p = p
                P(f, 'launched')
            end
            return p
        end
        table.insert(lanes, lane)
    end
    WaitTicks(5)
    for _, lane in lanes do
        lane.tml:GiveTacticalSiloAmmo(1)
        IssueTactical({lane.tml}, lane.site:GetPosition())
    end
    for _ = 1, 350 do
        WaitTicks(1)
        local flying = false
        for _, lane in lanes do
            if not lane.p then
                flying = true
            elseif not lane.done then
                if lane.p:BeenDestroyed() then
                    lane.done = true
                    P(lane.f, 'gone site ' .. (lane.site:BeenDestroyed() and 'dead' or string.format('%.0f', lane.site:GetHealth())))
                else
                    flying = true
                end
            end
        end
        if not flying then break end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
