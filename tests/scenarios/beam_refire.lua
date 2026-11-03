-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- A collision beam's far end (bone 1) is at its target as soon as it's
-- enabled, both the first time and on a refire at a new target: not where
-- it was made, nor where the last firing left it. FAF's `OnEnable` puts the
-- end-point emitters there (`CreateAttachedEmitter(beam, 1, ...)`), and the
-- renderer draws the beam to it. The Monkeylord's laser (continuous).
ForkThread(function()
    WaitTicks(2)
    local ml = CreateUnitHPR('url0402', 'ARMY_1', 400, ground(400, 600), 600, 0, 0, 0)
    local function tank(x, z)
        local t = CreateUnitHPR('uel0201', 'ARMY_2', x, ground(x, z), z, 0, 0, 0)
        IssueStop({t})
        t:SetFireState(1)
        -- Lives through the Monkeylord's other guns until the laser fires.
        t:SetMaxHealth(1e6)
        t:SetHealth(t, 1e6)
        return t
    end
    local w = ml:GetWeaponByLabel('MainGun')
    local beam = w and w.Beams and w.Beams[1] and w.Beams[1].Beam
    check(beam, 'the Monkeylord laser made its collision beam')
    if not beam then return done() end
    -- The end as `OnEnable` sees it, against the target the beam fires at.
    local enables = {}
    local on_enable = beam.OnEnable
    beam.OnEnable = function(self)
        local e = self:GetPosition(1)
        local t = w:GetCurrentTarget()
        local p = t and t:GetPosition()
        table.insert(enables, p and VDist2(e[1], e[3], p[1], p[3]) or -1)
        return on_enable(self)
    end
    local function wait_enables(n)
        for _ = 1, 150 do
            if table.getn(enables) >= n then return true end
            WaitTicks(1)
        end
    end
    local a = tank(415, 600)
    check(wait_enables(1), 'the laser fires at the first tank')
    a:Destroy()
    for _ = 1, 50 do
        if not beam:IsEnabled() then break end
        WaitTicks(1)
    end
    check(not beam:IsEnabled(), 'the laser stops once its target is gone')
    -- A new target on the other side: the refire.
    local b = tank(410, 615)
    local refired = wait_enables(2)
    check(refired, 'the laser refires at the second tank')
    if not refired then
        local t = w:GetCurrentTarget()
        note(string.format('no refire: second tank dead %s, laser target %s, enabled %s', tostring(b.Dead), tostring(t and t.UnitId), tostring(beam:IsEnabled())))
    end
    for i, d in enables do
        note(string.format('enable %d: end %.1f from the target', i, d))
        check(d >= 0 and d < 4, string.format('enable %d: the beam end starts at its target (%.1f away)', i, d))
    end
    done()
end)
