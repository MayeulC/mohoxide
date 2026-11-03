-- Differential probe: what a turret does with a target its turning limits
-- can't reach, and how a turret on another weapon's turret aims,
-- logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_turretreach 60
--   ours: cargo run -- --headless --no-ai --ticks 320 --script dev/probes/maps/probe_turretreach/probe.lua
-- A. The Fatboy (uel0401) at (420, 600) facing +z, invulnerable engineers on
--    its four diagonals 21 out: its side turrets' heading arcs cover a
--    diagonal their turning limits don't (LeftTurret01 wants ~117 degrees
--    against a limit of 80 in ours). Every 10 ticks per turret: its target
--    (diagonal index, '-' none), its aim heading, its shots so far.
-- B. A Cybran ACU (url0001) with the Microwave Laser Generator at (650, 160)
--    facing +z, an invulnerable tank 14 to its +x (90 degrees off its
--    facing, outside the laser's 30 degree turret range unless the torso
--    the gun turns carries it round). Every 10 ticks: the torso's facing,
--    the gun's and the laser's targets and aim headings, shots so far.
local TICKS = 300
local armies = ListArmies()
local function f2(n) return string.format('%.2f', n) end

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function spawn(bp, army, x, z)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    return u
end

-- Counts each weapon's shots (projectile weapons and beam weapons both
-- create through CreateProjectileAtMuzzle).
local function counter(unit, labels)
    local shots = {}
    for _, each in labels do
        -- (A fresh local per weapon: FA's Lua 5.0 shares the loop variable.)
        local label = each
        shots[label] = 0
        local w = unit:GetWeaponByLabel(label)
        if w then
            local create = w.CreateProjectileAtMuzzle
            w.CreateProjectileAtMuzzle = function(self, muzzle)
                shots[label] = shots[label] + 1
                return create(self, muzzle)
            end
        end
    end
    return shots
end

-- A.
local fx, fz = 420, 600
local fatboy = spawn('uel0401', armies[1], fx, fz)
IssueStop({fatboy})
local diagonals = {}
for i, o in {{21, 21}, {-21, 21}, {21, -21}, {-21, -21}} do
    local d = spawn('uel0105', armies[2], fx + o[1], fz + o[2])
    d:SetCanTakeDamage(false)
    diagonals[d:GetEntityId()] = i
end
local turrets = {'RightTurret01', 'RightTurret02', 'LeftTurret01', 'LeftTurret02'}
local fatboy_shots = counter(fatboy, turrets)

-- B.
local ax, az = 650, 160
local acu = spawn('url0001', armies[1], ax, az)
acu:CreateEnhancement('MicrowaveLaserGenerator')
local tank = spawn('uel0201', armies[2], ax + 14, az)
tank:SetCanTakeDamage(false)
local acu_shots = counter(acu, {'RightRipper', 'MLG'})

local function target_tag(w, tags)
    local t = w and w:GetCurrentTarget()
    if not t then return '-' end
    local id = t.GetEntityId and t:GetEntityId()
    return tostring(tags and id and tags[id] or 't')
end
local function heading_of(unit, label)
    local m = unit:GetWeaponManipulatorByLabel(label)
    if not m then return 'none' end
    local h = m:GetHeadingPitch()
    return f2(h)
end

ForkThread(function()
    for tick = 1, TICKS do
        WaitTicks(1)
        if math.mod(tick, 10) == 0 then
            for _, l in turrets do
                LOG('PROBE ', tick, ' fatboy ', l, ' target ', target_tag(fatboy:GetWeaponByLabel(l), diagonals), ' heading ', heading_of(fatboy, l), ' shots ', fatboy_shots[l])
            end
            LOG('PROBE ', tick, ' fatboy heading ', f2(fatboy:GetHeading()))
            local tx, ty, tz = acu:GetBoneDirection('Torso')
            LOG('PROBE ', tick, ' acu torso ', f2(math.atan2(tx, tz)),
                ' gun ', target_tag(acu:GetWeaponByLabel('RightRipper')), ' ', heading_of(acu, 'RightRipper'), ' shots ', acu_shots.RightRipper,
                ' laser ', target_tag(acu:GetWeaponByLabel('MLG')), ' ', heading_of(acu, 'MLG'), ' shots ', acu_shots.MLG)
        end
    end
    LOG('PROBE done')
end)
