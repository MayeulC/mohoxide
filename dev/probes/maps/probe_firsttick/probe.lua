-- Differential probe: what happens in a unit's first ticks (a Fire
-- Beetle created at the start next to an enemy fires on tick 0 in ours,
-- never in the real game).
--   real game (Windows): dev/probes/run_faf.ps1 probe_firsttick 120
--   ours: cargo run -- --headless --no-ai --ticks 100 --script dev/probes/maps/probe_firsttick/probe.lua
-- Armies: tested units in army 1, targets in NEUTRAL_CIVILIAN (made its
-- enemy, on hold fire, 30000 hp), the two armies no AI plays.
-- T: thread scheduling. A counter set after ForkThread returns shows whether
--    the body ran synchronously ('sync' = it saw the counter before); each
--    thread logs its first line and after WaitTicks(0) / WaitTicks(1).
--    Forked from the top-level script (tick 0), from a running thread at
--    tick 30, and from inside a weapon's OnFire (the weapon stage).
-- U: units (tank T1 uel0201 10 from its target, beetle xrl0302 3 from it):
--    'tank0' at the start; 'tank30' from the thread at tick 30; 'tankatk0'
--    at the start with IssueAttack on it at once; 'beetle0' / 'beetle30';
--    'beetleon0' / 'beetleon30' with the Suicide weapon forced on
--    (SetEnabled(true), then disables ignored); 'beetleatk0' / 'beetleatk30'
--    with IssueAttack on their target at once; 'tank5' / 'tank14' /
--    'tank47' from the thread at those ticks. 'target seen by army 1': the
--    first tick the lane's target's blip IsSeenNow(1).
-- Class hooks on the beetle and tank scripts log OnCreate begin / end,
-- OnStopBeingBuilt and the beetle's TrackTargetThread first line, with the
-- context ('ctx') they ran in: 'top' the top-level script, 'create:<tag>'
-- inside CreateUnitHPR, 'main' the probe's thread, 'eng' anything else
-- (engine stages).
-- Lines: PROBE <tick> <seq> <tag> <event> ...; seq orders events within a tick.
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

local seq = 0
local ctx = 'top'
local function P(tag, s, a, b, c, d)
    seq = seq + 1
    LOG(fmt('PROBE %d %d %s ', GetGameTick(), seq, tag) .. fmt(s, a, b, c, d))
end
P('start', 'ctx %s', ctx)

-- Flat land on SCMP_009 (from probe_auditgaps).
local LAND = {
    { 608, 80 }, { 688, 80 }, { 912, 96 }, { 784, 144 },
    { 656, 160 }, { 736, 224 }, { 816, 224 }, { 912, 240 },
    { 672, 304 }, { 800, 336 }, { 960, 336 }, { 656, 384 },
}

-- Class hooks: the unit scripts' module classes (the same table as an
-- instance's class, checked below).
local tagOf = {}
local function hookClass(path, name)
    local ok, mod = pcall(import, path)
    local cls = ok and mod and mod[name]
    if not cls then
        P('hook', '%s failed', name)
        return
    end
    local onCreate, onStop = cls.OnCreate, cls.OnStopBeingBuilt
    cls.OnCreate = function(self, a, b)
        P('oncreate', 'begin %s ctx %s', name, ctx)
        onCreate(self, a, b)
        P('oncreate', 'end %s ctx %s', name, ctx)
    end
    cls.OnStopBeingBuilt = function(self, a, b)
        P('onstop', '%s %s ctx %s', name, tagOf[self] or '?', ctx)
        onStop(self, a, b)
    end
    if cls.TrackTargetThread then
        local track = cls.TrackTargetThread
        cls.TrackTargetThread = function(self)
            P('track', 'first %s ctx %s', tagOf[self] or '?', ctx)
            return track(self)
        end
    end
    return cls
end
local classes = {
    uel0201 = hookClass('/units/uel0201/uel0201_script.lua', 'UEL0201'),
    xrl0302 = hookClass('/units/xrl0302/xrl0302_script.lua', 'XRL0302'),
}

local function make(bp, army, x, z)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
end

local units = {}
-- A unit and its target; weapon hooks; 'forced' keeps the first weapon on.
local function lane(tag, bp, site, dist, opts)
    opts = opts or {}
    local s = LAND[site]
    local target = make('uel0201', ENEMY, s[1], s[2] + dist)
    target:SetFireState(1)
    target:SetMaxHealth(30000) target:SetHealth(nil, 30000)
    local was = ctx
    ctx = 'create:' .. tag
    P(tag, 'create begin')
    local u = make(bp, TESTER, s[1], s[2])
    ctx = was
    tagOf[u] = tag
    P(tag, 'create end class %s', tostring(classes[bp] ~= nil and getmetatable(u) == classes[bp]))
    local w = { u = u, tag = tag, target = target, born = GetGameTick(), hp = target:GetHealth(), has = {}, dead = false }
    for i = 1, u:GetWeaponCount() do
        local wep = u:GetWeapon(i)
        local label = wep:GetBlueprint().Label
        P(tag, 'weapon %d %s', i, label)
        local forced = opts.forced and label == opts.forced
        local onFire = wep.OnFire
        wep.OnFire = function(self)
            P(tag, 'onfire %s ctx %s', label, ctx)
            if not w.forked then
                -- A thread forked in the weapon stage: when does it first run?
                w.forked = true
                local c = 0
                ForkThread(function()
                    P(tag, 'firethread first sync %s ctx %s', tostring(c == 0), ctx)
                    WaitTicks(1)
                    P(tag, 'firethread after WaitTicks(1)')
                end)
                c = 1
            end
            return getmetatable(self).OnFire(self)
        end
        local got = wep.OnGotTarget
        wep.OnGotTarget = function(self)
            P(tag, 'ongottarget %s ctx %s', label, ctx)
            return getmetatable(self).OnGotTarget(self)
        end
        local create = wep.CreateProjectileAtMuzzle
        if create then
            wep.CreateProjectileAtMuzzle = function(self, muzzle)
                P(tag, 'shot %s ctx %s', label, ctx)
                return create(self, muzzle)
            end
        end
        local setEnabled = wep.SetEnabled
        wep.SetEnabled = function(self, on)
            P(tag, 'setenabled %s %s ctx %s%s', label, tostring(on), ctx, forced and ' (ignored)' or '')
            if not forced then setEnabled(self, on) end
        end
        if forced then setEnabled(wep, true) end
        table.insert(w.has, { wep = wep, label = label, has = false })
    end
    if opts.attack then
        IssueAttack({ u }, target)
        P(tag, 'attack issued')
    end
    table.insert(units, w)
    return w
end

local function poll()
    for _, w in units do
        if not w.dead then
            if w.u.Dead or w.u:BeenDestroyed() then
                w.dead = true
                P(w.tag, 'dead')
            else
                for _, h in w.has do
                    local has = h.wep:WeaponHasTarget() and true or false
                    if has ~= h.has then
                        h.has = has
                        P(w.tag, 'hastarget %s %s', h.label, tostring(has))
                    end
                end
            end
        end
        if not w.seen then
            local ok, seen = pcall(function()
                local b = w.target:GetBlip(1)
                return b and b:IsSeenNow(1) or false
            end)
            if ok and seen then
                w.seen = true
                P(w.tag, 'target seen by army 1')
            elseif not ok and not w.seenErr then
                w.seenErr = true
                P(w.tag, 'seen check failed %s', tostring(seen))
            end
        end
        if not w.thp and w.target:GetHealth() < w.hp then
            w.thp = true
            P(w.tag, 'target damaged hp %.0f', w.target:GetHealth())
        end
    end
end

-- T: a thread forked from the top-level script.
local function threadTest(tag)
    local c = 0
    ForkThread(function()
        local was = ctx
        ctx = 'thread:' .. tag
        P(tag, 'first sync %s caller-ctx %s', tostring(c == 0), was)
        ctx = was
        WaitTicks(0)
        P(tag, 'after WaitTicks(0)')
        WaitTicks(1)
        P(tag, 'after WaitTicks(1)')
    end)
    c = 1
    P(tag, 'forkthread returned')
end
threadTest('T:top')

lane('tank0', 'uel0201', 1, 10)
lane('tankatk0', 'uel0201', 2, 10, { attack = true })
lane('beetle0', 'xrl0302', 3, 3)
lane('beetleon0', 'xrl0302', 4, 3, { forced = 'Suicide' })
lane('beetleatk0', 'xrl0302', 8, 3, { attack = true })
poll()

ForkThread(function()
    ctx = 'main'
    P('main', 'first')
    for i = 1, 90 do
        local t = GetGameTick()
        if t <= 8 or (t >= 29 and t <= 40) then P('main', 'tick') end
        if t == 5 then lane('tank5', 'uel0201', 9, 10) end
        if t == 14 then lane('tank14', 'uel0201', 10, 10) end
        if t == 30 then
            threadTest('T:30')
            lane('tank30', 'uel0201', 5, 10)
            lane('beetle30', 'xrl0302', 6, 3)
            lane('beetleon30', 'xrl0302', 7, 3, { forced = 'Suicide' })
            lane('beetleatk30', 'xrl0302', 11, 3, { attack = true })
        end
        if t == 47 then lane('tank47', 'uel0201', 12, 10) end
        poll()
        ctx = 'eng'
        WaitTicks(1)
        ctx = 'main'
    end
    P('main', 'done')
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
P('top', 'end')
ctx = 'eng'
