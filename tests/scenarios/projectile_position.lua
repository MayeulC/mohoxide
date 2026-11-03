-- Scenario test (tests/scenarios.rs): CreateProjectile with no offset puts
-- the projectile at the blueprint's Position* offset (the Cybran nuke's
-- plume, EMPFluxWarheadEffect01: PositionY 3); a given offset wins.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 570), 570, 0, 0, 0)
    local bp = '/effects/EMPFluxWarhead/EMPFluxWarheadEffect01_proj.bp'
    local y0 = tank:GetPosition()[2]
    local plume = tank:CreateProjectile(bp)
    local given = tank:CreateProjectile(bp, 0, 0, 0)
    local rise, zero = plume:GetPosition()[2] - y0, given:GetPosition()[2] - y0
    note(string.format('no offset: +%.2f, offset 0: +%.2f', rise, zero))
    check(math.abs(rise - 3) < 1e-3, 'no offset: 3 above the creator (got ' .. rise .. ')')
    check(math.abs(zero) < 1e-3, 'given offset 0: at the creator (got ' .. zero .. ')')
    done()
end)
