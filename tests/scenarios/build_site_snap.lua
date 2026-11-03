-- Scenario test (tests/scenarios.rs): a structure ordered built off the cell
-- grid stands where the real game puts it, its footprint on whole cells from
-- round(site - size / 2), halves to even (dev/probes probe_snap); one made by
-- CreateUnitHPR stays where it's made.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function done() ScenarioResult.done = true end
local function spawn(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
-- { blueprint, site x, z, where the real game put it x, z[, engineer x, z] }
local builds = {
    { 'ueb2101', 600, 380, 600.5, 380.5 },
    { 'ueb2101', 643.25, 382.75, 643.5, 382.5 },
    { 'ueb1101', 615, 395, 614.5, 394.5 },
    { 'ueb1101', 628.25, 394.75, 628.5, 394.5 },
    { 'ueb0101', 615, 423, 614.5, 422.5 },
    { 'ueb0103', 610.5, 291.5, 610, 291, 626, 288 },
}
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    local made = spawn('ueb0101', 628.25, 342.75)
    local mx, _, mz = made:GetPositionXYZ()
    check(mx == 628.25 and mz == 342.75, string.format('CreateUnitHPR at 628.25 342.75 stays (at %.2f %.2f)', mx, mz))
    local sites = {}
    for _, s in builds do
        local e = spawn('uel0105', s[6] or s[2] + 6, s[7] or s[3])
        IssueBuildMobile({ e }, { s[2], GetSurfaceHeight(s[2], s[3]), s[3] }, s[1], {})
        table.insert(sites, { s = s, e = e })
    end
    for _ = 1, 150 do
        WaitTicks(1)
        for _, site in sites do
            local u = not site.at and site.e:GetFocusUnit()
            if u and u.UnitId == site.s[1] then
                local x, _, z = u:GetPositionXYZ()
                site.at = { x, z }
            end
        end
    end
    for _, site in sites do
        local s = site.s
        local at = site.at
        check(at and at[1] == s[4] and at[2] == s[5], string.format('%s ordered at %.2f %.2f stands at %.2f %.2f (at %s)',
            s[1], s[2], s[3], s[4], s[5], at and string.format('%.2f %.2f', at[1], at[2]) or 'never started'))
    end
    done()
end)
