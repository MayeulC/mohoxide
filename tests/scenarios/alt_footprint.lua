-- Scenario test (tests/scenarios.rs): unit:ForceAltFootprint. The Cybran
-- destroyer's weapon toggle (URS0201_script.lua:176) keeps it on water: sent
-- inland, it stops at the shore; toggled back it walks out.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function wet(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) > 3 end
ForkThread(function()
    WaitTicks(2)
    -- Deep water with dry land 40 to its east.
    local wx, wz
    for x = 32, 960, 8 do
        for z = 32, 992, 8 do
            if wet(x, z) and wet(x + 8, z) and not wet(x + 40, z) and GetTerrainHeight(x + 40, z) - GetTerrainHeight(x + 30, z) < 3 then wx, wz = x, z break end
        end
        if wx then break end
    end
    if not wx then
        table.insert(ScenarioResult.notes, 'no shore on this map: skipped')
        ScenarioResult.done = true
        return
    end
    local ship = CreateUnitHPR('urs0201', 'ARMY_1', wx, GetSurfaceHeight(wx, wz), wz, 0, 0, 0)
    WaitTicks(2)
    local ok, err = pcall(ship.SetScriptBit, ship, 'RULEUTC_WeaponToggle', true)
    check(ok, 'toggling runs: ' .. tostring(err))
    local errors = 0
    IssueMove({ ship }, { wx + 40, GetTerrainHeight(wx + 40, wz), wz })
    WaitTicks(300)
    local p = ship:GetPosition()
    check(wet(p[1], p[3]) or GetSurfaceHeight(p[1], p[3]) > GetTerrainHeight(p[1], p[3]), string.format('kept to the water: %.1f, %.1f', p[1], p[3]))
    IssueClearCommands({ ship })
    ship:SetScriptBit('RULEUTC_WeaponToggle', false)
    IssueMove({ ship }, { wx + 40, GetTerrainHeight(wx + 40, wz), wz })
    WaitTicks(400)
    p = ship:GetPosition()
    check(VDist2(p[1], p[3], wx + 40, wz) < 4, string.format('toggled back, it walks ashore: %.1f, %.1f', p[1], p[3]))
    ScenarioResult.done = true
end)
