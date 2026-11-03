-- Scenario test (tests/scenarios.rs): ships, and hover units over water,
-- killed on water sink (FAF's sinker projectile) and are removed once they
-- reach the bottom: none stays around, dead, under the water or the map.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        -- Open water on the default map: the deepest of a coarse search.
        local w, h = GetMapSize()
        local best, bx, bz = 1e9, 0, 0
        for x = 20, w - 20, 16 do
            for z = 20, h - 20, 16 do
                local g = GetTerrainHeight(x, z)
                if g < best then best, bx, bz = g, x, z end
            end
        end
        local surface = GetSurfaceHeight(bx, bz)
        note(string.format('water at %d, %d: seabed %.1f, surface %.1f', bx, bz, best, surface))
        local ships = {}
        for i, bp in { 'ues0103', 'ues0201', 'uas0103', 'urs0103', 'ual0201', 'ual0101', 'ual0105' } do
            table.insert(ships, CreateUnitHPR(bp, 'ARMY_1', bx + (i - 2) * 12, surface, bz, 0, 0, 0))
        end
        WaitTicks(10)
        for _, s in ships do s:Kill() end
        local gone_at = {}
        for tick = 1, 600 do
            WaitTicks(1)
            for i, s in ships do
                if not gone_at[i] and IsDestroyed(s) then gone_at[i] = tick end
            end
        end
        for i, s in ships do
            local where = IsDestroyed(s) and 'removed' or string.format('still there at %.1f (seabed %.1f)', s:GetPosition()[2], GetTerrainHeight(s:GetPosition()[1], s:GetPosition()[3]))
            note(string.format('%s: %s%s', s.UnitId or '?', where, gone_at[i] and (' after ' .. gone_at[i] .. ' ticks') or ''))
            check(IsDestroyed(s), (s.UnitId or '?') .. ' was removed after sinking')
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
