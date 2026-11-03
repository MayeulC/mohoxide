-- Differential probe: does an ordinary beam (not missile defence) hit
-- projectiles? Logged in a format both engines print identically.
--   real game (Windows): dev/probes/run_faf.ps1 probe_beamhit 40
--   ours: cargo run -- --headless --no-ai --ticks 260 --script dev/probes/maps/probe_beamhit/probe.lua
-- Two lanes, a Cybran Monkeylord (army 1, only its `MainGun` laser on)
-- facing +z with an enemy 25 ahead (facing it) firing back down the beam:
-- A. a UEF T1 tank (shells, no health);
-- B. a Cybran T1 mobile missile launcher (missiles with health).
-- Everything is invulnerable. Per lane, at the end: the beam's impacts by
-- type (`OnImpact`), the enemy's shots, and how its projectiles ended
-- (their `OnImpact` type).
local armies = ListArmies()

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function make(bp, army, x, z, heading)
    -- (CreateUnitHPR's angles are pitch, yaw, roll.)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
    u:SetCanTakeDamage(false)
    return u
end
local function count(t, k) t[k] = (t[k] or 0) + 1 end
local function show(t)
    local keys = {}
    for k in t do table.insert(keys, k) end
    table.sort(keys)
    local s = ''
    for _, k in keys do s = s .. ' ' .. k .. '=' .. t[k] end
    return s == '' and ' none' or s
end

local TICKS = 200
ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    local lanes = {}
    for _, l in {{'A', 'uel0201', 100.5}, {'B', 'url0111', 300.5}} do
        local name, enemyBp, x = l[1], l[2], l[3]
        local lane = { name = name, beam = {}, shots = 0, ends = {} }
        local ml = make('url0402', armies[1], x, 700.5)
        for _, label in {'RightLaserTurret', 'LeftLaserTurret', 'RightAntiAirMissile', 'LeftAntiAirMissile', 'Torpedo'} do
            ml:SetWeaponEnabledByLabel(label, false)
        end
        local enemy = make(enemyBp, armies[2], x, 725.5, math.pi)
        local gun = ml:GetWeaponByLabel('MainGun')
        for _, info in gun.Beams or {} do
            local beam = info.Beam
            local onImpact = beam.OnImpact
            beam.OnImpact = function(self, kind, other)
                count(lane.beam, kind)
                return onImpact(self, kind, other)
            end
        end
        local w = enemy:GetWeapon(1)
        local create = w.CreateProjectileAtMuzzle
        w.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            if p then
                lane.shots = lane.shots + 1
                local onImpact = p.OnImpact
                p.OnImpact = function(proj, kind, other)
                    count(lane.ends, kind)
                    return onImpact(proj, kind, other)
                end
            end
            return p
        end
        -- (Both see the lane: the launcher's vision is 18.)
        for _, a in {1, 2} do
            import('/lua/sim/VizMarker.lua').VizMarker({ X = x, Z = 712.5, Radius = 30, LifeTime = -1, Army = a,
                Omni = false, Radar = false, Vision = true, WaterVision = true })
        end
        IssueAttack({ml}, enemy)
        IssueAttack({enemy}, ml)
        table.insert(lanes, lane)
    end
    WaitTicks(TICKS)
    for _, lane in lanes do
        LOG('PROBE ', lane.name, ' beam impacts', show(lane.beam))
        LOG('PROBE ', lane.name, ' enemy shots ', lane.shots, ' ended', show(lane.ends))
    end
    LOG('PROBE done')
end)
