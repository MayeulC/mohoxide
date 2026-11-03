-- Development scene for particle effects, on the land patch of scmp_009:
-- FA's medium explosion template every two seconds on a static unit, a light
-- flash, a beam between two units, and a tank duel at close range (muzzle
-- flashes on the aimed barrels, shell trails, impacts).
-- Run: cargo run -- --script dev/scenes/visual/effects.lua --camera 412,598,24 (duel: 404,590,8)
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end

local EffectTemplate = import('/lua/effecttemplates.lua')

-- Explosions on a static target (a wall segment; never killed).
local target = spawn('ueb5101', 1, 412, 600)
target:SetCanTakeDamage(false)

-- A beam between two point-defence turrets' muzzles (same army: they
-- don't shoot each other).
local a = spawn('ueb2101', 1, 426, 596)
local b = spawn('ueb2101', 1, 434, 600)
for _, u in {a, b} do u:SetCanTakeDamage(false) end

-- A tank duel across the camera, facing each other.
spawn('uel0201', 1, 398, 590, math.pi / 2)
spawn('ual0201', 2, 410, 590, -math.pi / 2)

ForkThread(function()
    while true do
        for _, bp in EffectTemplate.ExplosionMedium do
            CreateEmitterAtEntity(target, 1, bp)
        end
        CreateLightParticle(target, -1, 1, 4, 6, 'glow_02', 'ramp_flare_02')
        -- The beam's blueprint lives 10 ticks; refire it like a beam weapon.
        CreateBeamEntityToEntity(a, 'Turret_Muzzle', b, 'Turret_Muzzle', 1, '/effects/emitters/microwave_laser_beam_01_emit.bp')
        WaitSeconds(1)
    end
end)
