-- Scenario test (tests/scenarios.rs): hovering aircraft climb under moho's
-- vertical controller (force = KLift x height error - KLiftDamping x
-- vertical speed). Two UEF gunships (uea0203) differ only in KLift; both
-- start 2 above the ground and climb to their Elevation: the stiffer one
-- gets there sooner, and GetVelocity reports the climb.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function spawn(x, klift)
    local air = __blueprints['uea0203'].Air
    local old = air.KLift
    air.KLift = klift
    local u = CreateUnitHPR('uea0203', 'ARMY_1', x, GetTerrainHeight(x, 580) + 2, 580, 0, 0, 0)
    -- (Scripts' aircraft start at their flying height: put it down low.)
    Warp(u, { x, GetTerrainHeight(x, 580) + 2, 580 })
    air.KLift = old
    return u
end
ForkThread(function()
    WaitTicks(2)
    local soft, stiff = spawn(390, 0.5), spawn(410, 4)
    local h = function(u) local p = u:GetPosition() return p[2] - GetTerrainHeight(p[1], p[3]) end
    WaitTicks(5)
    local _, vy = stiff:GetVelocity()
    WaitTicks(5)
    note(string.format('after 1 s: KLift 0.5 at %.2f, KLift 4 at %.2f; climbing at %.2f/s', h(soft), h(stiff), vy * 10))
    check(h(stiff) > h(soft) + 0.5, 'the stiffer controller climbs faster')
    check(vy > 0, 'GetVelocity reports the climb')
    ScenarioResult.done = true
end)
