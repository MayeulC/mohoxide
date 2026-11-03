-- Scenario test (tests/scenarios.rs): weapon aim. One shooter (ARMY_1) fires
-- at one target (ARMY_2, holding fire, too tough to die) per trial: standing
-- still, or driving / flying back and forth across the line of fire, at a
-- few ranges. Every shot is followed to its impact; the trial logs shots,
-- direct hits and how close the shots came (closest approach of the shot's
-- path to the target's centre, 0 for a hit), e.g.
--   aim: uel0201 vs uel0201 r=12 moving: 10 shots, 9 hits (90%), mean closest 0.12
-- and each shot's launch error against where the target was at the time.
-- Run alone: cargo run --release -- --headless --no-ai --ticks 12000 --script tests/scenarios/aim.lua
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) LOG('aim: ', msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end

-- Dead flat land on the default map (bumps of a world unit stop low shots).
local CX, CZ = 384, 896

-- shooter bp, weapon label, target bp, range, moving (half-length of the
-- target's run across the line of fire at that range, or false), minimum hit
-- rate, shots to take, rate of fire override (slow artillery), air target,
-- bearing (degrees off the shooter's facing the target stands at; weapons
-- slaved to the body must turn the unit), rate by damaging shots instead of
-- direct hits (splash, ground-seeking missiles).
local trials = {
    { 'uel0201', 'MainGun', 'uel0201', 8, false, 0.9 },
    { 'uel0201', 'MainGun', 'uel0201', 16, false, 0.9 },
    { 'uel0201', 'MainGun', 'uel0201', 8, 12, 0.7 },
    { 'uel0201', 'MainGun', 'uel0201', 12, 12, 0.6 },
    { 'url0107', 'LaserArms', 'uel0201', 12, false, 0.9 },
    { 'url0107', 'LaserArms', 'uel0201', 12, 12, 0.6 },
    { 'ual0201', 'MainGun', 'uel0201', 22, false, 0.9 },
    { 'ual0201', 'MainGun', 'uel0201', 18, 16, 0.5 },
    { 'uel0103', 'MainGun', 'uel0201', 20, false, 0.5, 8, 1 },
    -- Artillery at a moving tank: 8 shots, 1-2 hits depending on the draws.
    { 'uel0103', 'MainGun', 'uel0201', 20, 18, 0.1, 8, 1 },
    { 'uel0104', 'AAGun', 'uea0203', 20, false, 0.8, 12, nil, true },
    { 'uel0104', 'AAGun', 'uea0203', 20, 20, 0.5, 12, nil, true },
    { 'uel0104', 'AAGun', 'uea0101', 20, 20, 0.3, 12, nil, true },
    -- Hull-fixed: the Aurora's gun turns 10 degrees each way; the unit
    -- itself must face a target off to the side (SlavedToBody).
    { 'ual0201', 'MainGun', 'uel0201', 18, false, 0.9, 6, nil, false, 60 },
    { 'ual0201', 'MainGun', 'uel0201', 18, false, 0.9, 6, nil, false, 150 },
    { 'uel0203', 'Riotgun01', 'uel0201', 12, false, 0.6, 6, nil, false, -90 },
    -- Homing anti-air missiles (TrackTarget) at aircraft crossing back and
    -- forth: the T3 SAM's Flayers, the Cybran T1 mobile AA's darts.
    { 'ueb2304', 'MissileRack01', 'uea0101', 30, 25, 0.8, 8, nil, true },
    { 'ueb2304', 'MissileRack01', 'uea0103', 30, 25, 0.8, 8, nil, true },
    { 'url0104', 'AAGun', 'uea0101', 14, 15, 0.6, 12, nil, true },
    -- Aircraft circling the launcher (moving = 'circle'): the missiles
    -- must turn inside the target's turn.
    { 'ueb2304', 'MissileRack01', 'uea0101', 25, 'circle', 0.8, 8, nil, true },
    { 'ueb2304', 'MissileRack01', 'uea0102', 25, 'circle', 0.8, 8, nil, true },
    { 'url0104', 'AAGun', 'uea0101', 14, 'circle', 0.6, 12, nil, true },
    -- Interceptors left idle circle tightly in place (moving = 'idle').
    { 'url0104', 'AAGun', 'ura0102', 14, 'idle', 0.6, 12, nil, true },
    { 'ueb2304', 'MissileRack01', 'ura0102', 25, 'idle', 0.8, 8, nil, true },
    -- Zig-zagging missiles (blueprint MaxZigZag / ZigZagFrequency): the
    -- Cybran T3 SAM's (5 units, a new offset every 0.5 s) at crossing
    -- aircraft, the mobile missile launchers' (Evensong 4, Viper and
    -- Ythisah 1) at a tank.
    { 'urb2304', 'Missile01', 'uea0101', 30, 25, 0.8, 8, nil, true },
    { 'urb2304', 'Missile01', 'uea0103', 30, 25, 0.8, 8, nil, true },
    -- Mobile missile launchers at a power generator and a tank: reported,
    -- not checked, except the Ythisah's. Their racks aren't turrets, and
    -- our shots leave along the aim, flat, instead of up the tilted rack:
    -- the missiles skim the ground and zig-zag into it (issue #1). The Evensong's go for the ground under the target
    -- (`TrackTargetGround`): rated by shots that damage it. The Ythisah's
    -- hit rate swings 67-100% with the random draws (the trials before it
    -- shift them): 12 shots, at least half.
    { 'ual0111', 'MissileRack', 'ueb1101', 40, false, 0, 4, 1, false, nil, true },
    { 'ual0111', 'MissileRack', 'uel0201', 40, false, 0, 4, 1, false, nil, true },
    { 'url0111', 'MissileRack', 'ueb1101', 40, false, 0, 4, 1, false, nil, true },
    { 'url0111', 'MissileRack', 'uel0201', 40, false, 0, 4, 1, false, nil, true },
    { 'xsl0111', 'MissileRack', 'ueb1101', 40, false, 0.5, 12, 1 },
}

-- Set AIM_FILTER (a Lua pattern on the shooter's blueprint id) first to run
-- only some trials.
local filter = rawget(_G, 'AIM_FILTER')

local function run(t)
    local sbp, label, tbp, range, moving, minRate, wantShots, rof, air, bearing, byDamage = unpack(t)
    wantShots = wantShots or 10
    local what = string.format('%s vs %s r=%d %s', sbp, tbp, range, moving and (type(moving) == 'string' and moving or 'moving') or (bearing and ('still at ' .. bearing .. ' deg') or 'still'))
    local shooter = CreateUnitHPR(sbp, 'ARMY_1', CX, ground(CX, CZ), CZ, 0, 0, 0)
    local b = math.rad(bearing or 0)
    local tx, tz = CX + range * math.sin(b), CZ + range * math.cos(b)
    local ty = air and ground(tx, tz) + 18 or ground(tx, tz)
    local circle, idle = moving == 'circle', moving == 'idle'
    local target = CreateUnitHPR(tbp, 'ARMY_2', (moving and not circle and not idle) and tx + moving or tx, ty, tz, 0, 0, 0)
    target:SetMaxHealth(1e7)
    target:SetHealth(nil, 1e7)
    target:SetFireState(1)
    shooter:SetFireState(1)
    if idle then
        -- A short hop, then left to its own devices (aircraft circle).
        IssueMove({ target }, { tx + 3, ty, tz })
    elseif circle then
        -- Round and round the shooter at the trial's range.
        for i = 1, 64 do
            local a = math.rad(i * 45)
            local px, pz = CX + range * math.sin(a), CZ + range * math.cos(a)
            IssueMove({ target }, { px, ground(px, pz) + 18, pz })
        end
    elseif air and not moving then
        -- Kept in the air on its spot: an idle aircraft lands after its
        -- AutoLandTime (a second for gunships), and holding fire it has no
        -- weapon target to circle instead.
        ForkThread(function()
            while not target:BeenDestroyed() do
                IssueClearCommands({ target })
                IssueMove({ target }, { tx, ty, tz })
                WaitTicks(5)
            end
        end)
    elseif moving then
        -- Back and forth across the line of fire.
        IssueMove({ target }, { tx - moving, ty, tz })
        for i = 1, 12 do
            IssueMove({ target }, { tx + (i % 2 == 1 and moving or -moving), ty, tz })
        end
    end
    -- Let a mover reach speed (and aircraft settle) before firing.
    WaitTicks(moving and 30 or 10)
    shooter:SetFireState(0)
    local w = shooter:GetWeaponByLabel(label)
    if not w then
        check(false, sbp .. ' has no weapon ' .. label)
        shooter:Destroy() target:Destroy() WaitTicks(10)
        return
    end
    if rof then w:ChangeRateOfFire(rof) end
    -- Target centre height above its position.
    local tbpt = target:GetBlueprint()
    local lift = (tbpt.SizeY or 1) / 2 + (tbpt.CollisionOffsetY or 0)
    local function centre()
        local q = target:GetPosition()
        return { q[1], q[2] + lift, q[3] }
    end
    -- Closest approach of each shot's path (per-tick segments) to the target
    -- centre; a direct hit counts 0.
    local live, shots, hits, impacts, missSum = {}, 0, 0, 0, 0
    -- Shots that damaged the target (direct or splash, any fragment).
    local damaging, damageSum = 0, 0
    local wanderSum = 0
    -- Time from fire-enabled to the first shot, and each hit's flight time.
    local start, firstShot, flightSum = GetGameTick(), nil, 0
    local function closest(a, b, c)
        local dx, dy, dz = b[1] - a[1], b[2] - a[2], b[3] - a[3]
        local len2 = dx * dx + dy * dy + dz * dz
        local f = len2 > 0 and ((c[1] - a[1]) * dx + (c[2] - a[2]) * dy + (c[3] - a[3]) * dz) / len2 or 0
        f = math.max(0, math.min(1, f))
        return VDist3({ a[1] + dx * f, a[2] + dy * f, a[3] + dz * f }, c)
    end
    local function track(rec)
        local p = rec.proj:GetPosition()
        local c = centre()
        local d = closest(rec.last, p, c)
        if d < rec.best then rec.best = d end
        -- Off the straight line from the launch point to the target (how
        -- far zig-zag, or an arc, takes it), across the line of flight.
        local ax, az = c[1] - rec.start[1], c[3] - rec.start[3]
        local len = math.sqrt(ax * ax + az * az)
        if len > 0 then
            local off = math.abs((p[1] - rec.start[1]) * az - (p[3] - rec.start[3]) * ax) / len
            if off > rec.wander then rec.wander = off end
        end
        rec.last = p
    end
    local function finish(rec, kind, other)
        if rec.done then return end
        rec.done = true
        impacts = impacts + 1
        if not rec.proj:BeenDestroyed() then track(rec) end
        if other == target or rec.fragmentHit then
            hits = hits + 1
            rec.best = 0
            flightSum = flightSum + (GetGameTick() - rec.t0)
        end
        missSum = missSum + rec.best
        wanderSum = wanderSum + rec.wander
        if rec.damage > 0 then
            damaging = damaging + 1
            damageSum = damageSum + rec.damage
        end
        LOG(string.format('aim shot %s #%d: %s closest %.2f, %d fragments, damage %d; at launch: heading off the target %.1f deg, pitch %.1f deg, target %.1f away at %.1f/s',
            sbp, rec.n, kind, rec.best, table.getn(rec.children), rec.damage, rec.err, rec.pitch, rec.dist, rec.tspeed))
    end
    local orig = w.CreateProjectileForWeapon
    w.CreateProjectileForWeapon = function(self, ...)
        local proj = orig(self, ...)
        if proj and not proj:BeenDestroyed() and shots < wantShots then
            shots = shots + 1
            local rec = { proj = proj, n = shots, best = 1e9, last = proj:GetPosition(), t0 = GetGameTick() }
            firstShot = firstShot or (GetGameTick() - start)
            -- Launch heading against the target's centre now (degrees, the
            -- lead a moving target needs shows up here).
            local vx, vy, vz = proj:GetVelocity()
            local p, c = proj:GetPosition(), centre()
            local err = math.deg(math.atan2(vx, vz) - math.atan2(c[1] - p[1], c[3] - p[3]))
            if err > 180 then err = err - 360 elseif err < -180 then err = err + 360 end
            rec.err = err
            rec.pitch = math.deg(math.atan2(vy, math.sqrt(vx * vx + vz * vz)))
            rec.dist = VDist2(p[1], p[3], c[1], c[3])
            local tvx, _, tvz = target:GetVelocity()
            rec.tspeed = math.sqrt(tvx * tvx + tvz * tvz) * 10
            table.insert(live, rec)
            rec.out, rec.children, rec.damage = 0, {}, 0
            rec.start, rec.wander = proj:GetPosition(), 0
            -- Follows the shot and any fragments it splits into (the
            -- Lobo's shell, FAF's script `CreateChildProjectile`s at the
            -- detonation height): the shot is over when every piece has
            -- impacted, and hits if one of them hit the target.
            local function follow(p)
                rec.out = rec.out + 1
                local ccp = p.CreateChildProjectile
                p.CreateChildProjectile = function(pr, bp)
                    local child = ccp(pr, bp)
                    table.insert(rec.children, child)
                    follow(child)
                    return child
                end
                local oi, impacted = p.OnImpact, false
                p.OnImpact = function(pr, kind, other)
                    if pr ~= rec.proj and other == target then rec.fragmentHit = true end
                    local before = target:GetHealth()
                    local r = oi(pr, kind, other)
                    rec.damage = rec.damage + math.max(0, before - target:GetHealth())
                    if not impacted then
                        impacted = true
                        rec.out = rec.out - 1
                        if rec.out == 0 then finish(rec, kind, other) end
                    end
                    return r
                end
            end
            follow(proj)
        end
        return proj
    end
    for _ = 1, 1200 do
        WaitTicks(1)
        for _, rec in live do
            if not rec.done then
                if not rec.proj:BeenDestroyed() then
                    track(rec)
                else
                    local pieces = false
                    for _, c in rec.children do
                        if not c:BeenDestroyed() then pieces = true end
                    end
                    if not pieces then finish(rec, 'gone', nil) end
                end
            end
        end
        if impacts >= wantShots then break end
    end
    local rate = shots > 0 and (byDamage and damaging or hits) / shots or 0
    note(string.format('%s: %d shots, %d hits (%d%%), %d damaging (mean %d), mean closest %.2f, mean sideways wander %.2f, first shot after %.1fs, mean flight to a hit %.1fs', what, shots, hits,
        math.floor(hits / math.max(shots, 1) * 100 + 0.5), damaging, damaging > 0 and damageSum / damaging or 0,
        impacts > 0 and missSum / impacts or -1, impacts > 0 and wanderSum / impacts or -1, (firstShot or -10) / 10, hits > 0 and flightSum / hits / 10 or -1))
    check(shots >= wantShots, string.format('%s: fired %d of %d shots', what, shots, wantShots))
    check(rate >= minRate, string.format('%s: %s rate %d%% below %d%%', what, byDamage and 'damaging' or 'hit', math.floor(rate * 100 + 0.5), math.floor(minRate * 100)))
    shooter:Destroy()
    target:Destroy()
    WaitTicks(10)
end

ForkThread(function()
    WaitTicks(2)
    -- Radar so the shooter's army sees past its own vision. It runs on
    -- energy: with none stored FAF's energy manager turns it off.
    for _, army in { 'ARMY_1', 'ARMY_2' } do
        local b = GetArmyBrain(army)
        b:__engine_SetStorage('ENERGY', 500000) b:GiveResource('ENERGY', 500000)
    end
    CreateUnitHPR('ueb3101', 'ARMY_1', CX - 30, ground(CX - 30, CZ - 20), CZ - 20, 0, 0, 0)
    -- A far-off unit keeps ARMY_2 in the game between trials.
    CreateUnitHPR('ueb3101', 'ARMY_2', CX + 200, ground(CX + 200, CZ - 300), CZ - 300, 0, 0, 0)
    for _, t in trials do
        if not filter or string.find(t[1], filter) then run(t) end
    end
    -- An aircraft killed in the air falls and crashes (`OnImpact`), and
    -- weapons stop shooting at it on the way down (a dying plane takes no
    -- hits: FAF turns its collisions off).
    local sam = CreateUnitHPR('ueb2304', 'ARMY_1', CX, ground(CX, CZ), CZ, 0, 0, 0)
    local plane = CreateUnitHPR('ura0102', 'ARMY_2', CX, ground(CX, CZ + 20) + 18, CZ + 20, 0, 0, 0)
    plane:SetFireState(1)
    IssueMove({ plane }, { CX + 30, ground(CX + 30, CZ + 20) + 18, CZ + 20 })
    WaitTicks(20)
    local impacted = false
    local oi = plane.OnImpact
    plane.OnImpact = function(self, with)
        impacted = with
        return oi(self, with)
    end
    sam:SetFireState(1)
    plane:Kill()
    local start = GetGameTick()
    WaitTicks(2)
    local w = sam:GetWeaponByLabel('MissileRack01')
    local shotAt = false
    for _ = 1, 100 do
        if w:GetCurrentTarget() == plane then shotAt = true end
        if impacted then break end
        WaitTicks(1)
    end
    note(string.format('killed interceptor: crashed (%s) after %.1fs, still targeted on the way down: %s',
        tostring(impacted), (GetGameTick() - start) / 10, tostring(shotAt)))
    check(impacted == 'Terrain', 'a killed aircraft never crashed (OnImpact)')
    check(not shotAt, 'a killed aircraft is still a weapon target')
    sam:Destroy()
    done()
end)
