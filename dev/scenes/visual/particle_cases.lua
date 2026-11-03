-- Particle cases on the land patch of scmp_009, one per row, each a unit
-- firing at an invulnerable target so the effects repeat for as long as the
-- run lasts. Run with --no-ai; look at each row with --camera:
--
--   triad      ueb2301 vs a unit:   polytrail, munition glows, gauss hits     --camera 364,545,16
--   ravager    xeb2306 vs a unit:   15-round plasma gatling salvo, flashes    --camera 370,620,18
--   aurora     ual0201 vs a unit:   SinglePolyTrail disruptor shot            --camera 375,560,18
--   flak       uel0205 vs a scout:  SingleCompositeEmitter: beam + polytrail
--                                   with LODCutoff -1 + fx trail             --camera 378,575,22
--   gunship    uea0203 vs a unit:   BeamExhaust beams + riot gun             --camera 378,590,20
--   artillery  ual0304 vs a unit:   sonance hits (LODCutoff -1 emitters)     --camera 400,650,25
--   explosion  ExplosionMedium + CreateLightParticle on a wall               --camera 360,660,24
--   torpedo    ueb2109 vs a sub:    underwater sort order (-102) bubbles     --camera 480,600,25
--
-- Screenshot: cargo run -- --script dev/scenes/visual/particle_cases.lua --no-ai --screenshot x.png --screenshot-tick 150 --camera 378,545,30
local function spawn(bp, army, x, z, heading, alt)
    return CreateUnitHPR(bp, army, x, GetSurfaceHeight(x, z) + (alt or 0), z, 0, heading or 0, 0)
end

local function target(bp, army, x, z, alt)
    local u = spawn(bp, army, x, z, 0, alt)
    u:SetCanTakeDamage(false)
    return u
end

local EffectTemplate = import('/lua/effecttemplates.lua')

-- Targets are enemy engineers (no weapon, so they don't shoot back;
-- walls aren't auto-targeted), made invulnerable.
-- Triad (UEF T2 point defence) vs an engineer 22 away (range 50, vision 28).
spawn('ueb2301', 1, 360, 545)
target('ual0105', 2, 382, 545)

-- Ravager (UEF T3 point defence) vs an engineer 22 away (range 70, vision 24).
spawn('xeb2306', 1, 360, 620)
target('ual0105', 2, 382, 620)

-- Aurora (Aeon T1 tank) vs an engineer 20 away (range 26).
spawn('ual0201', 1, 365, 560, math.pi / 2)
target('ual0105', 2, 385, 560)

-- UEF T1 mobile AA vs an Aeon scout hovering 25 away (AA range 40).
spawn('uel0205', 1, 365, 575, math.pi / 2)
target('uaa0101', 2, 390, 575, 10)

-- UEF T1 gunship vs an engineer (range 22): hovers, exhaust beams, riot gun.
spawn('uea0203', 1, 370, 590, math.pi / 2, 8)
target('ual0105', 2, 385, 590)

-- Aeon T3 mobile artillery vs an engineer 56 away (range 25..90).
spawn('ual0304', 1, 360, 610, math.pi / 4)
target('ual0105', 2, 400, 650)

-- A medium explosion and a light flash every second on a wall.
local boom = target('ueb5101', 1, 360, 660)
ForkThread(function()
    while true do
        for _, bp in EffectTemplate.ExplosionMedium do
            CreateEmitterAtEntity(boom, 1, bp)
        end
        CreateLightParticle(boom, -1, 1, 4, 6, 'glow_02', 'ramp_flare_02')
        WaitSeconds(1)
    end
end)

-- UEF torpedo launcher on the water vs an Aeon submarine 20 away (range 50).
spawn('ueb2109', 1, 470, 600)
target('uas0203', 2, 490, 600)

-- Effects spawned directly, so they're always on screen:
--   sonance hits (LODCutoff -1 emitters) on a wall every second        --camera 380,530,16
--   an Aurora shot beam from `CreateBeamEmitterOnEntity` (blueprint Length)
--   Aeon bomber hit (ParticleResistance drag) on the next wall         --camera 400,530,16
local sonance = target('ueb5101', 1, 376, 530)
local beam_host = target('ueb5101', 1, 384, 530)
local dragged = target('ueb5101', 1, 400, 530)
ForkThread(function()
    while true do
        for _, bp in EffectTemplate.ASonanceWeaponHit02 do
            CreateEmitterAtEntity(sonance, 1, bp)
        end
        CreateBeamEmitterOnEntity(beam_host, -1, 1, '/effects/emitters/cannon_munition_ship_aeon_beam_01_emit.bp'):ScaleEmitter(6)
        for _, bp in EffectTemplate.ABombHit01 do
            CreateEmitterAtEntity(dragged, 1, bp)
        end
        WaitSeconds(1)
    end
end)
