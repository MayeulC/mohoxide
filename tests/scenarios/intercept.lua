-- Scenario test (tests/scenarios.rs): missile defence. An army 2 tactical
-- missile launched at an army 1 power generator guarded by a UEF TMD
-- (TargetType RULEWTT_Projectile, OnlyAllow TACTICAL MISSILE) is shot
-- down: the TMD targets the missile, its led shot hits it, and the
-- generator is untouched. Unguarded, the same missile lands.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function make(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
local function fire(tml, at)
    tml:GiveTacticalSiloAmmo(1)
    IssueTactical({tml}, at:GetPosition())
end
ForkThread(function()
    WaitTicks(2)
    local guarded = make('ueb1101', 'ARMY_1', 400.5, 600.5)
    -- Right beside its charge: TML missiles take two Phalanx hits, one reload
    -- apart.
    local tmd = make('ueb4201', 'ARMY_1', 406.5, 600.5)
    local open = make('ueb1101', 'ARMY_1', 400.5, 680.5)
    local tml = make('ueb2108', 'ARMY_2', 500.5, 640.5)
    local intercepted = 0
    local origI = tml.OnMissileIntercepted
    tml.OnMissileIntercepted = function(self, ...) intercepted = intercepted + 1 if origI then return origI(self, ...) end end
    local tmdFired = false
    local w = tmd:GetWeapon(1)
    -- (Not OnFire: FAF swaps weapon methods per state.)
    local origF = w.CreateProjectileForWeapon
    w.CreateProjectileForWeapon = function(self, ...) tmdFired = true return origF(self, ...) end
    WaitTicks(5)
    local full = guarded:GetHealth()
    fire(tml, guarded)
    for _ = 1, 400 do
        WaitTicks(1)
        if intercepted > 0 then break end
    end
    check(tmdFired, 'the TMD fired at the missile')
    check(intercepted == 1, 'the missile was intercepted: ' .. intercepted)
    WaitTicks(20)
    check(guarded:GetHealth() == full, string.format('guarded generator untouched (%.0f / %.0f)', guarded:GetHealth(), full))
    local openFull = open:GetHealth()
    fire(tml, open)
    for _ = 1, 400 do
        WaitTicks(1)
        if open:BeenDestroyed() or open:GetHealth() < openFull then break end
    end
    check(open:BeenDestroyed() or open:GetHealth() < openFull, 'an unguarded target is hit')

    -- The TML's missiles, to see where they end.
    local missile
    local tw = tml:GetWeapon(1)
    local create = tw.CreateProjectileAtMuzzle
    tw.CreateProjectileAtMuzzle = function(self, muzzle) missile = create(self, muzzle) return missile end
    -- Where it was last seen, and the damage it took.
    local function follow(p)
        local at, damage = nil, 0
        local onDamage = p.OnDamage
        p.OnDamage = function(self, instigator, amount, ...)
            damage = damage + amount
            return onDamage(self, instigator, amount, ...)
        end
        for _ = 1, 300 do
            if p:BeenDestroyed() then break end
            at = p:GetPosition()
            WaitTicks(1)
        end
        return at, damage
    end

    -- A Cybran TMD zaps the missile with its beam (FAF probe probe_tmd: a
    -- 1 HP Cybran missile killed ~28 out, as it comes into its 31 range).
    -- This UEF one has 2 HP; both zap ticks of one shot hit it (~28 out).
    -- Our beams once passed through projectiles: the missile flew on into
    -- its target. (On dry land: a muzzle under the water surface ends the
    -- beam there; and off the UEF TMD's path, which would shoot it down
    -- first.)
    local cyb = make('ueb1101', 'ARMY_1', 600.5, 460.5)
    local zapper = make('urb4201', 'ARMY_1', 606.5, 460.5)
    WaitTicks(5)
    local cybFull = cyb:GetHealth()
    missile = nil
    fire(tml, cyb)
    while not missile do WaitTicks(1) end
    local at, damage = follow(missile)
    WaitTicks(20)
    check(not cyb:BeenDestroyed() and cyb:GetHealth() == cybFull, 'the Cybran TMD saved its charge')
    check(damage > 0, 'the zapper hurt the missile')
    local d = at and VDist2(at[1], at[3], 606.5, 460.5) or 0
    check(d > 15, string.format('zapped down as it came into range: %.1f out', d))

    -- An Aeon TMD fires a flare whose spheres redirect the missile onto it
    -- (FAF's Flare). It unpacks first: the shot the pass decided on the
    -- tick it took its target is taken back once FAF's unpacking state
    -- disables it (else its 4 s reload started on a shot never fired), and
    -- the flare's spheres, moving onto the missile, catch it.
    local aeon = make('ueb1101', 'ARMY_1', 520.5, 500.5)
    make('uab4201', 'ARMY_1', 526.5, 500.5)
    WaitTicks(5)
    local aeonFull = aeon:GetHealth()
    missile = nil
    fire(tml, aeon)
    while not missile do WaitTicks(1) end
    local redirected
    for _ = 1, 300 do
        if missile:BeenDestroyed() then break end
        redirected = redirected or missile.IsRedirected
        WaitTicks(1)
    end
    WaitTicks(20)
    check(redirected, 'an Aeon flare redirected the missile')
    check(not aeon:BeenDestroyed() and aeon:GetHealth() == aeonFull, 'the Aeon TMD saved its charge')

    -- A flare (script entity with a collision sphere, FAF's
    -- defaultantiprojectile Flare) is asked about the missile flying into it.
    local decoy = make('ueb1101', 'ARMY_1', 400.5, 840.5)
    decoy.RedirectedMissiles = 0
    local flare = import('/lua/defaultantiprojectile.lua').Flare { Owner = decoy, Radius = 10 }
    local asked
    local origC = flare.OnCollisionCheck
    flare.OnCollisionCheck = function(self, other) asked = other return origC(self, other) end
    WaitTicks(5)
    fire(tml, decoy)
    for _ = 1, 300 do
        WaitTicks(1)
        if asked then break end
    end
    check(asked ~= nil and IsProjectile(asked), 'the flare was asked about the missile')

    -- A nuke at a base with strategic missile defence (OnlyAllow STRATEGIC
    -- MISSILE; the TMD ignores it) is shot down by the SMD's interceptor.
    local smd = make('ueb4302', 'ARMY_1', 300.5, 400.5)
    local site = make('ueb1101', 'ARMY_1', 310.5, 400.5)
    local silo = make('ueb2305', 'ARMY_2', 600.5, 400.5)
    local nukeDown = 0
    local origN = silo.OnMissileIntercepted
    silo.OnMissileIntercepted = function(self, ...) nukeDown = nukeDown + 1 if origN then return origN(self, ...) end end
    WaitTicks(5)
    -- Its interceptors are counted (tactical silo ammo), built beforehand.
    smd:GiveTacticalSiloAmmo(1)
    silo:GiveNukeSiloAmmo(1)
    IssueNuke({silo}, site:GetPosition())
    for _ = 1, 900 do
        WaitTicks(1)
        if nukeDown > 0 or site:BeenDestroyed() then break end
    end
    check(nukeDown == 1, 'the nuke was intercepted: ' .. nukeDown)
    check(not site:BeenDestroyed(), 'the defended site survives')
    ScenarioResult.done = true
end)
