-- Differential probe: does a structure's turret swing round to a
-- target off its nose at TurretYawSpeed before its first shot?
--   real game (Windows): dev/probes/run_faf.ps1 probe_turretslew 150
--   ours: cargo run -- --headless --no-ai --ticks 420 --script dev/probes/maps/probe_turretslew/probe.lua
-- Each lane: a structure in army 1 created facing +z and, as FAF turns a new
-- weapon structure to the nearest enemy or the map centre
-- (StructureUnit.RotateTowardsEnemy), turned back to heading 0 about world
-- up (the weapon audit does the same). At tick ORDER its target (a T1 power
-- generator in NEUTRAL_CIVILIAN, made its enemy, 1e6 hp) is created at
-- bearing b (0 / 90 / 180, + towards +x) DIST away, with an army-1 power
-- generator beside it for sight, and the structure is ordered to attack it.
-- Lanes: uab2101 (TurretYawSpeed 180, DIST 18), xsb2303 and ueb2303 (6, DIST
-- 70). Every STEP ticks until a few after the first shot: the weapon
-- manipulator's GetHeadingPitch. Lines: PROBE <tick> <tag> <event> ...;
-- 'spawn heading <deg>' (after CreateUnitHPR, before turning back), 'order',
-- 'aim <heading> <pitch>', 'shot' (CreateProjectileAtMuzzle), every 40
-- ticks 'state hp <target hp> hastarget <b> seen <b> queue <n>'.
local fmt = string.format
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local armies = ListArmies()
local TESTER = armies[1]
local ENEMY = 'NEUTRAL_CIVILIAN'
SetAlliance(TESTER, ENEMY, 'Enemy')
SetAlliance(ENEMY, TESTER, 'Enemy')
local ORDER = 20
local TICKS = 400

local function P(tag, s, a, b, c, d)
    LOG(fmt('PROBE %d %s ', GetGameTick(), tag) .. fmt(s, a, b, c, d))
end

-- Flat land on SCMP_009 (probe_auditgaps' scan).
local LAND = {
    { 608, 80 }, { 688, 80 }, { 912, 96 }, { 784, 144 },
    { 656, 160 }, { 736, 224 }, { 816, 224 }, { 912, 240 },
    { 672, 304 }, { 800, 336 }, { 960, 336 }, { 656, 384 },
    { 880, 400 }, { 576, 464 }, { 384, 560 }, { 96, 608 },
    { 352, 640 }, { 64, 688 }, { 176, 688 }, { 320, 720 },
    { 224, 768 }, { 112, 784 }, { 304, 800 }, { 240, 880 },
    { 352, 880 }, { 96, 896 }, { 432, 912 },
}
local used = {}
local function dry(x, z)
    return x > 16 and z > 16 and x < 1008 and z < 1008 and GetTerrainHeight(x, z) >= GetSurfaceHeight(x, z)
end
-- the first unused site whose target spot (and the way there) is dry land
local function site(b, dist)
    local ux, uz = math.sin(math.rad(b)), math.cos(math.rad(b))
    for i, s in LAND do
        if not used[i] then
            local ok = true
            for k = 1, 4 do
                if not dry(s[1] + ux * dist * k / 4, s[2] + uz * dist * k / 4) then ok = false end
            end
            if ok then
                used[i] = true
                return s[1], s[2], s[1] + ux * dist, s[2] + uz * dist
            end
        end
    end
end

local lanes = {}
local function lane(bp, b, dist, step)
    local tag = bp .. '@' .. b
    local x, z, tx, tz = site(b, dist)
    if not x then
        P(tag, 'no site')
        return
    end
    local u = CreateUnitHPR(bp, TESTER, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    local h = u:GetHeading()
    P(tag, 'spawn heading %.2f at %d %d', math.deg(h), x, z)
    if h ~= 0 then
        local cs, sn = math.cos(-h / 2), math.sin(-h / 2)
        local q1, q2, q3, q4 = unpack(u:GetOrientation())
        u:SetOrientation({ cs * q1 - sn * q3, cs * q2 + sn * q4, cs * q3 + sn * q1, cs * q4 - sn * q2 }, true)
    end
    P(tag, 'heading now %.2f', math.deg(u:GetHeading()))
    local wep = u:GetWeapon(1)
    local label = wep:GetBlueprint().Label
    local l = { tag = tag, u = u, wep = wep, label = label, tx = tx, tz = tz, step = step, shots = 0 }
    local create = wep.CreateProjectileAtMuzzle
    wep.CreateProjectileAtMuzzle = function(self, muzzle)
        l.shots = l.shots + 1
        if l.shots <= 2 then
            P(tag, 'shot %d', l.shots)
            if not l.first then l.first = GetGameTick() end
        end
        return create(self, muzzle)
    end
    table.insert(lanes, l)
end

lane('uab2101', 0, 18, 1)
lane('uab2101', 90, 18, 1)
lane('uab2101', 180, 18, 1)
lane('xsb2303', 0, 70, 10)
lane('xsb2303', 90, 70, 10)
lane('xsb2303', 180, 70, 10)
lane('ueb2303', 0, 70, 10)
lane('ueb2303', 90, 70, 10)
lane('ueb2303', 180, 70, 10)

local function aim(l)
    local ok, hd, pt = pcall(function()
        return l.u:GetWeaponManipulatorByLabel(l.label):GetHeadingPitch()
    end)
    if ok and hd then
        P(l.tag, 'aim %.1f %.1f', math.deg(hd), math.deg(pt or 0))
    elseif not l.aimErr then
        l.aimErr = true
        P(l.tag, 'aim unreadable %s', tostring(hd))
    end
end

ForkThread(function()
    for i = 1, TICKS do
        local t = GetGameTick()
        if t == ORDER then
            for _, l in lanes do
                local y = GetTerrainHeight(l.tx, l.tz)
                local target = CreateUnitHPR('uab1101', ENEMY, l.tx, y, l.tz, 0, 0, 0)
                target:SetMaxHealth(1e6) target:SetHealth(nil, 1e6)
                CreateUnitHPR('uab1101', TESTER, l.tx + 3, GetTerrainHeight(l.tx + 3, l.tz), l.tz, 0, 0, 0)
                l.target = target
                IssueAttack({ l.u }, target)
                P(l.tag, 'order')
            end
        end
        if t > ORDER and math.mod(t - ORDER, 40) == 0 then
            -- every 40 ticks: target hp, has-target, seen by army 1, queue
            for _, l in lanes do
                if l.target and not l.target:BeenDestroyed() then
                    local ok, seen = pcall(function() local b = l.target:GetBlip(1) return b and b:IsSeenNow(1) or false end)
                    P(l.tag, 'state hp %.0f hastarget %s seen %s queue %d', l.target:GetHealth(),
                        tostring(l.wep:WeaponHasTarget() and true or false), tostring(ok and seen), table.getn(l.u:GetCommandQueue() or {}))
                end
            end
        end
        if t >= ORDER then
            for _, l in lanes do
                if not l.u:BeenDestroyed() and (not l.first or t <= l.first + 3) and math.mod(t - ORDER, l.step) == 0 then
                    aim(l)
                end
            end
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
