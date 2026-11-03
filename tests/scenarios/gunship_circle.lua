-- Scenario test (tests/scenarios.rs): hovering aircraft attacking drift
-- round their target (moho CalcCirclingOrientation through the KMove PD
-- controller): a UEF gunship stays at 50-90% of its weapon range, keeps
-- moving slowly (a unit or two a second) and reverses now and then - the
-- side-to-side strafing of the real game, not laps. The same round an
-- attack-ground point. The CZAR (HoverOverAttack) holds over its target.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end

-- Watches `g` round `centre()` for `ticks`: distance band, arc travelled,
-- direction reversals, mean speed (units/s).
local function watch(g, centre, ticks)
    local min, max, swept, last, lastsign, flips, path = 1e9, 0, 0, nil, 0, 0, 0
    local prev = g:GetPosition()
    for _ = 1, ticks do
        local p, t = g:GetPosition(), centre()
        local d = VDist2(p[1], p[3], t[1], t[3])
        min, max = math.min(min, d), math.max(max, d)
        local a = math.atan2(p[1] - t[1], p[3] - t[3])
        if last then
            local da = a - last
            if da > math.pi then da = da - 2 * math.pi elseif da < -math.pi then da = da + 2 * math.pi end
            swept = swept + math.abs(da)
            local sign = (da > 0.002 and 1) or (da < -0.002 and -1) or 0
            if sign ~= 0 and lastsign ~= 0 and sign ~= lastsign then flips = flips + 1 end
            if sign ~= 0 then lastsign = sign end
        end
        last = a
        path = path + VDist2(p[1], p[3], prev[1], prev[3])
        prev = p
        WaitTicks(1)
    end
    return min, max, math.deg(swept), flips, path / ticks * 10
end

ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('ual0201', 'ARMY_2', 420, GetTerrainHeight(420, 600), 600, 0, 0, 0)
    tank:SetCanTakeDamage(false)
    local g = CreateUnitHPR('uea0203', 'ARMY_1', 380, GetTerrainHeight(380, 600) + 20, 600, 0, 0, 0)
    IssueAttack({g}, tank)
    local range = 0
    for _, w in g.Blueprint.Weapon do range = math.max(range, w.MaxRadius or 0) end
    WaitTicks(100)
    local min, max, swept, flips, speed = watch(g, function() return tank:GetPosition() end, 300)
    note(string.format('range %.1f, distance %.1f..%.1f, swept %.0f deg, %d reversals, %.1f/s', range, min, max, swept, flips, speed))
    check(min > range * 0.4 and max < range * 1.1, string.format('stays within its range (%.1f..%.1f of %.1f)', min, max, range))
    check(swept > 20, string.format('keeps moving round the target (%.0f deg)', swept))
    check(flips >= 1, 'reverses direction (strafes back and forth)')
    check(speed < 4, string.format('drifts slowly, not laps at full speed (%.1f/s)', speed))

    -- Attack-ground: the same round the point.
    g:Destroy()
    local g2 = CreateUnitHPR('uea0203', 'ARMY_1', 380, GetTerrainHeight(380, 640) + 20, 640, 0, 0, 0)
    local spot = { 420, GetTerrainHeight(420, 640), 640 }
    IssueAttack({g2}, spot)
    WaitTicks(120)
    local gmin, gmax, gswept, gflips, gspeed = watch(g2, function() return spot end, 300)
    note(string.format('attack-ground: distance %.1f..%.1f, swept %.0f deg, %d reversals, %.1f/s', gmin, gmax, gswept, gflips, gspeed))
    check(gmax < range * 1.1 and gswept > 20, string.format('drifts round an attack-ground point (%.0f deg, %.1f..%.1f)', gswept, gmin, gmax))
    g2:Destroy()

    local czar = CreateUnitHPR('uaa0310', 'ARMY_1', 470, GetTerrainHeight(470, 600) + 30, 650, 0, 0, 0)
    IssueAttack({czar}, tank)
    WaitTicks(250)
    local c, t = czar:GetPosition(), tank:GetPosition()
    note(string.format('czar %.1f from its target', VDist2(c[1], c[3], t[1], t[3])))
    check(VDist2(c[1], c[3], t[1], t[3]) < 3, 'the CZAR holds over its target')
    ScenarioResult.done = true
end)
