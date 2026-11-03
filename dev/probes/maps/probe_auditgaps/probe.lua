-- Differential probe: gaps from the audit, one scenario per question, each
-- at its own site (far enough apart that nothing reaches another).
-- Logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_auditgaps 300
--   ours: cargo run -- --headless --no-ai --ticks 1000 --script dev/probes/maps/probe_auditgaps/probe.lua
-- Armies: everything tested in army 1 and every target in NEUTRAL_CIVILIAN
-- (made its enemy), the two armies no AI plays in the real game (its other
-- armies are AI: they give the units orders). Targets are on hold fire.
-- G1 overcharge: per faction, 4 commanders, one lane each; the 16 lanes
--    run one at a time (50 ticks each, 'auto' 80) with the energy topped up
--    to 1000000 before each: 'tough' (T1 tank with 30000 hp,
--    IssueOverCharge), 'normal' (T1 tank, IssueOverCharge), 'bldg' (T1 power
--    generator with 30000 hp, IssueOverCharge), 'auto' (T1 tank with 30000
--    hp, SetAutoOvercharge(true), no order). Target 15 away.
-- G2: xrb2309 and xrb2308 created at the water surface in deep water.
-- G4: torpedo bombers (and the CZAR) created 40 east of their target, in
--    two waves at the same 5 sites: a surfaced frigate (IssueAttack at the
--    start), then a dived T1 sub (from tick 320, IssueAttack once it's down).
--    Targets 30000 hp.
-- G5: Fire Beetle by a T1 tank (30000 hp): 'auto4.5' no order; 'hold4.5' /
--    'hold3' on hold fire, IssueAttack; 'atk20' IssueAttack (walks);
--    'move20' IssueMove onto it; 'atk4.5' / 'atk3' IssueAttack.
-- G6: hold fire (SetFireState(1)) and an explicit order: T1 tank
--    IssueAttack; T1 tank no order (control); TML IssueTactical; TML
--    IssueAttack (target 30 away, 1 missile).
-- G7: T1 sub created at the water surface: layer, then IssueDive twice
--    (each once the last change of layer has settled).
-- Lines: PROBE <tick> <tag> <event> ...; 'shot <label>' a projectile left a
-- weapon, 'dmg <amount> <type> by <bp> hp <after>', 'dead', 'queue <n>'
-- (command queue length changed), 'energy <stored>', 'layer <layer>'.
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local fmt = string.format
local TESTER = armies[1]
local ENEMY = 'NEUTRAL_CIVILIAN'
local enemyIndex
for i, a in armies do
    if a == ENEMY then enemyIndex = i end
end
SetAlliance(TESTER, ENEMY, 'Enemy')
SetAlliance(ENEMY, TESTER, 'Enemy')
local FACTIONS = { 'uel0001', 'ual0001', 'url0001', 'xsl0001' }
local brain = ArmyBrains[1]

-- Sites, from a scan of SCMP_009 at 16 steps, 72 apart: flat land (height
-- range under 3 within 20) and water deeper than 12 within 20.
local LAND = {
    -- G1: 16 lanes (faction x lane), G5: 5, G6: 4
    { 608, 80 }, { 688, 80 }, { 912, 96 }, { 784, 144 },
    { 656, 160 }, { 736, 224 }, { 816, 224 }, { 912, 240 },
    { 672, 304 }, { 800, 336 }, { 960, 336 }, { 656, 384 },
    { 880, 400 }, { 576, 464 }, { 384, 560 }, { 96, 608 },
    { 352, 640 }, { 64, 688 }, { 176, 688 }, { 320, 720 },
    { 224, 768 }, { 112, 784 }, { 304, 800 }, { 240, 880 },
    { 352, 880 }, { 96, 896 }, { 432, 912 },
}
local WATER = {
    -- G2: 2, G7: 1, G4: 5 (at least 150 apart)
    { 48, 48 }, { 128, 48 }, { 208, 48 },
    { 208, 128 }, { 448, 320 }, { 176, 320 }, { 768, 656 }, { 624, 608 },
}

local function make(bp, army, x, z, y)
    return CreateUnitHPR(bp, army, x, y or GetTerrainHeight(x, z), z, 0, 0, 0)
end
local function tough(u, hp)
    u:SetMaxHealth(hp) u:SetHealth(nil, hp)
end
local function bpid(e)
    if not e then return 'nil' end
    local ok, id = pcall(function() return e:GetBlueprint().BlueprintId end)
    return ok and tostring(id) or '?'
end

-- Watching: damage taken (OnDamage), deaths, projectiles fired, queue length, layer.
local watched = {}
local function watch(u, tag, opts)
    local w = { u = u, tag = tag, q = -1, layer = opts and opts.layer and '' or nil, dmg = 0, n = 0, total = 0 }
    table.insert(watched, w)
    local on = u.OnDamage
    u.OnDamage = function(self, inst, amount, vec, kind)
        on(self, inst, amount, vec, kind)
        w.n = w.n + 1
        w.total = w.total + amount
        if w.n <= 12 then
            LOG(fmt('PROBE %d %s dmg %.1f %s by %s hp %.1f', GetGameTick(), tag, amount, tostring(kind), bpid(inst), self:GetHealth()))
        end
    end
    return w
end
local function shots(u, tag)
    for i = 1, u:GetWeaponCount() do
        local wep = u:GetWeapon(i)
        local label = wep:GetBlueprint().Label
        local create = wep.CreateProjectileAtMuzzle
        if create then
            wep.CreateProjectileAtMuzzle = function(self, muzzle)
                local p = create(self, muzzle)
                LOG(fmt('PROBE %d %s shot %s', GetGameTick(), tag, label))
                return p
            end
        end
    end
end
local function pollWatched()
    for _, w in watched do
        if not w.dead then
            if w.u.Dead or w.u:BeenDestroyed() then
                w.dead = true
                LOG(fmt('PROBE %d %s dead', GetGameTick(), w.tag))
            else
                local q = table.getn(w.u:GetCommandQueue() or {})
                if q ~= w.q then
                    w.q = q
                    LOG(fmt('PROBE %d %s queue %d', GetGameTick(), w.tag, q))
                end
                if w.layer then
                    local l = w.u:GetCurrentLayer()
                    if l ~= w.layer then
                        w.layer = l
                        local p = w.u:GetPosition()
                        LOG(fmt('PROBE %d %s layer %s y %.2f surf %.2f', GetGameTick(), w.tag, l, p[2], GetSurfaceHeight(p[1], p[3])))
                    end
                end
            end
        end
    end
end

do
    local s = ''
    for i, a in armies do s = s .. fmt(' %s/%s', a, tostring(ArmyBrains[i].BrainType)) end
    LOG('PROBE start ', GetGameTick(), ' enemy ', tostring(IsEnemy(1, enemyIndex)), ' ', tostring(IsEnemy(enemyIndex, 1)), s)
end
brain:GiveStorage('ENERGY', 1000000)
brain:GiveStorage('MASS', 10000)
-- (The real game's storage grows on its next economy tick, and GiveResource
-- clamps to the storage there is: top up from tick 2 on.)
local function topUp()
    local e = brain:GetEconomyStored('ENERGY')
    brain:GiveResource('ENERGY', 1000000 - e)
    return e
end

-- G1 overcharge
local LANES = { 'tough', 'normal', 'bldg', 'auto' }
local g1 = {}
local g1Start0 = 20
for fi, f in FACTIONS do
    for li, lane in LANES do
        local s = LAND[(fi - 1) * 4 + li]
        local acu = make(f, TESTER, s[1], s[2])
        local tag = f .. ':' .. lane
        watch(acu, tag)
        shots(acu, tag)
        table.insert(g1, { acu = acu, tag = tag, lane = lane, li = li, s = s })
    end
end
-- One lane at a time: lane order by type, then faction.
table.sort(g1, function(a, b) return a.li < b.li or (a.li == b.li and a.tag < b.tag) end)
do
    local t = g1Start0
    for _, l in g1 do
        l.st = t
        l.en = t + (l.lane == 'auto' and 80 or 50)
        t = l.en + 2
    end
end
local function g1Start(l)
    local x, z = l.s[1], l.s[2] + 15
    local target
    if l.lane == 'bldg' then
        target = make('ueb1101', ENEMY, x + 0.5, z + 0.5)
    else
        target = make('uel0201', ENEMY, x, z)
        target:SetFireState(1)
    end
    if l.lane ~= 'normal' then tough(target, 30000) end
    l.target = target
    watch(target, l.tag .. '>tgt')
    if l.lane == 'auto' then
        l.acu:SetAutoOvercharge(true)
    else
        IssueOverCharge({ l.acu }, target)
    end
    LOG(fmt('PROBE %d %s start energy %.0f hp %.0f', GetGameTick(), l.tag, brain:GetEconomyStored('ENERGY'), target:GetHealth()))
end
local function g1Stop(l)
    LOG(fmt('PROBE %d %s stop energy %.0f target %s', GetGameTick(), l.tag, brain:GetEconomyStored('ENERGY'),
        (l.target.Dead or l.target:BeenDestroyed()) and 'dead' or fmt('hp %.0f', l.target:GetHealth())))
    if l.lane == 'auto' then l.acu:SetAutoOvercharge(false) end
    IssueClearCommands({ l.acu })
    if not l.target:BeenDestroyed() then l.target:Destroy() end
end
local lastEnergy

-- G2 HARMS
local g2 = {}
for i, bp in { 'xrb2309', 'xrb2308' } do
    local s = WATER[i]
    LOG(fmt('PROBE %d g2 site %s depth %.2f surf %.2f', GetGameTick(), bp, GetSurfaceHeight(s[1], s[2]) - GetTerrainHeight(s[1], s[2]), GetSurfaceHeight(s[1], s[2])))
    local u = make(bp, TESTER, s[1] + 0.5, s[2] + 0.5, GetSurfaceHeight(s[1], s[2]))
    if not u:BeenDestroyed() then u:SetFireState(1) end
    local p = u:GetPosition()
    LOG(fmt('PROBE %d g2 %s created id %s layer %s y %.2f dead %s', GetGameTick(), bp, u:GetEntityId(), u:GetCurrentLayer(), p[2], tostring(u:BeenDestroyed())))
    table.insert(g2, { bp = bp, s = s })
end
local function g2Scan()
    for _, g in g2 do
        local s = ''
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            local p = u:GetPosition()
            if not u:BeenDestroyed() and VDist2(p[1], p[3], g.s[1], g.s[2]) < 10 then
                s = s .. fmt(' %s/%s/%s/y%.2f/hp%.0f/fs%d', bpid(u), u:GetEntityId(), u:GetCurrentLayer(), p[2], u:GetHealth(), u:GetFireState())
            end
        end
        LOG(fmt('PROBE %d g2 %s near:%s', GetGameTick(), g.bp, s))
    end
end

-- G7 sub created at the surface
local g7s = WATER[3]
local g7 = make('ues0203', TESTER, g7s[1], g7s[2], GetSurfaceHeight(g7s[1], g7s[2]))
g7:SetFireState(1)
watch(g7, 'g7:ues0203', { layer = true })

-- G4 torpedo bombers
local BOMBERS = { 'uaa0204', 'ura0204', 'xaa0306', 'xsa0204', 'uaa0310' }
local g4 = {}
local function g4Targets(kind)
    for bi, bp in BOMBERS do
        local s = WATER[3 + bi]
        local t = make(kind == 'frigate' and 'ues0103' or 'ues0203', ENEMY, s[1], s[2], GetSurfaceHeight(s[1], s[2]))
        t:SetFireState(1)
        tough(t, 30000)
        local tag = 'g4:' .. bp .. '>' .. kind
        watch(t, tag, { layer = true })
        table.insert(g4, { bp = bp, kind = kind, t = t, s = s, tag = tag })
    end
end
local function g4Order(kind)
    for _, g in g4 do
        if g.kind == kind then
            local b = make(g.bp, TESTER, g.s[1] + 40, g.s[2], GetSurfaceHeight(g.s[1] + 40, g.s[2]) + 20)
            local tag = 'g4:' .. g.bp .. '@' .. g.kind
            watch(b, tag)
            shots(b, tag)
            IssueAttack({ b }, g.t)
            g.b = b
            LOG(fmt('PROBE %d %s ordered target layer %s', GetGameTick(), tag, g.t:GetCurrentLayer()))
        end
    end
end
local function g4Clear(kind)
    for _, g in g4 do
        if g.kind == kind then
            LOG(fmt('PROBE %d %s end target %s', GetGameTick(), g.tag,
                (g.t.Dead or g.t:BeenDestroyed()) and 'dead' or fmt('hp %.0f', g.t:GetHealth())))
            for _, u in { g.t, g.b } do
                if u and not u:BeenDestroyed() then u:Destroy() end
            end
        end
    end
end
g4Targets('frigate')

-- G5 Fire Beetle
local g5 = {}
for i, c in { { 'auto4.5', 4.5 }, { 'hold4.5', 4.5 }, { 'hold3', 3 }, { 'atk20', 20 }, { 'move20', 20 }, { 'atk4.5', 4.5 }, { 'atk3', 3 } } do
    local site = i <= 5 and 16 + i or 20 + i
    local s = LAND[site]
    local tank = make('uel0201', ENEMY, s[1], s[2])
    tank:SetFireState(1)
    tough(tank, 30000)
    local beetle = make('xrl0302', TESTER, s[1], s[2] - c[2])
    if string.find(c[1], 'hold') then beetle:SetFireState(1) end
    local tag = 'g5:' .. c[1]
    watch(tank, tag .. '>tgt')
    watch(beetle, tag)
    shots(beetle, tag)
    table.insert(g5, { kind = c[1], tank = tank, beetle = beetle, tag = tag })
end

-- G6 hold fire
local g6 = {}
for i, c in { { 'tank-atk', 'uel0201', 15 }, { 'tank-none', 'uel0201', 15 }, { 'tml-tac', 'ueb2108', 30 }, { 'tml-atk', 'ueb2108', 30 } } do
    local s = LAND[21 + i]
    local shooter = make(c[2], TESTER, s[1] + 0.5, s[2] + 0.5)
    shooter:SetFireState(1)
    local target = c[2] == 'ueb2108' and make('ueb1101', ENEMY, s[1] + 0.5, s[2] + c[3] + 0.5) or make('uel0201', ENEMY, s[1], s[2] + c[3])
    target:SetFireState(1)
    tough(target, 30000)
    local tag = 'g6:' .. c[1]
    watch(shooter, tag)
    shots(shooter, tag)
    watch(target, tag .. '>tgt')
    table.insert(g6, { kind = c[1], shooter = shooter, target = target, tag = tag })
end

ForkThread(function()
    local t0 = GetGameTick()
    local g4Ordered = false
    local g7Phase = 'rest'
    local g7Tick, g7From
    WaitTicks(1)
    for _, g in g6 do
        LOG(fmt('PROBE %d %s firestate %d', GetGameTick(), g.tag, g.shooter:GetFireState()))
        if g.kind == 'tank-atk' then IssueAttack({ g.shooter }, g.target)
        elseif g.kind == 'tml-tac' then g.shooter:GiveTacticalSiloAmmo(1) IssueTactical({ g.shooter }, g.target)
        elseif g.kind == 'tml-atk' then g.shooter:GiveTacticalSiloAmmo(1) IssueAttack({ g.shooter }, g.target) end
    end
    g4Order('frigate')
    for _, g in g5 do
        if g.kind == 'move20' then IssueMove({ g.beetle }, g.tank:GetPosition())
        elseif g.kind ~= 'auto4.5' and not g.beetle:BeenDestroyed() then IssueAttack({ g.beetle }, g.tank) end
    end
    for i = 1, 960 do
        local tick = GetGameTick() - t0
        if tick == 2 then topUp() end
        for _, l in g1 do
            if tick == l.st - 1 then
                topUp()
            elseif tick == l.st then
                g1Start(l)
            elseif tick == l.en then
                g1Stop(l)
            end
        end
        local e = brain:GetEconomyStored('ENERGY')
        if not lastEnergy or math.abs(e - lastEnergy) > 3000 then
            lastEnergy = e
            LOG(fmt('PROBE %d energy %.0f', GetGameTick(), e))
        end
        -- G2: every tick for 5, then every 50.
        if tick <= 5 or math.mod(tick, 50) == 0 then g2Scan() end
        -- G7: IssueDive at 10, again once the layer has changed and settled.
        local settled = not g7:IsUnitState('MovingDown') and not g7:IsUnitState('MovingUp')
        if tick == 10 or (g7Phase == 'first' and g7:GetCurrentLayer() ~= g7From and settled) then
            LOG(fmt('PROBE %d g7 dive from %s%s', GetGameTick(), g7:GetCurrentLayer(),
                g7Tick and fmt(' (changed after %d)', GetGameTick() - g7Tick) or ''))
            g7From = g7:GetCurrentLayer()
            IssueDive({ g7 })
            g7Phase = tick == 10 and 'first' or 'second'
            g7Tick = GetGameTick()
        elseif g7Phase == 'second' and g7:GetCurrentLayer() ~= g7From and settled then
            LOG(fmt('PROBE %d g7 back to %s after %d', GetGameTick(), g7:GetCurrentLayer(), GetGameTick() - g7Tick))
            g7Phase = 'done'
        end
        -- G4: frigate wave until 310; sub wave from 320, ordered once the subs are down.
        if tick == 310 then g4Clear('frigate') end
        if tick == 320 then
            g4Targets('sub')
        elseif tick == 321 then
            for _, g in g4 do
                if g.kind == 'sub' then
                    LOG(fmt('PROBE %d %s initial layer %s', GetGameTick(), g.tag, g.t:GetCurrentLayer()))
                    if g.t:GetCurrentLayer() ~= 'Sub' then IssueDive({ g.t }) end
                end
            end
        elseif tick > 321 and not g4Ordered then
            local down = true
            for _, g in g4 do
                if g.kind == 'sub' and (g.t:GetCurrentLayer() ~= 'Sub' or g.t:IsUnitState('MovingDown')) then down = false end
            end
            if down or tick >= 420 then
                g4Ordered = true
                g4Order('sub')
            end
        end
        pollWatched()
        WaitTicks(1)
    end
    for _, w in watched do
        if w.n > 0 then LOG(fmt('PROBE total %s hits %d damage %.1f', w.tag, w.n, w.total)) end
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
