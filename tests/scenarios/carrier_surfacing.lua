-- Scenario test (tests/scenarios.rs): a submerged carrier surfaces on its
-- own. Moho's carrier retrieve task surfaces a `SurfacingSub`
-- carrier in the `Sub` layer once its load order is under way and takes the
-- aircraft in only once it's up (MohoEngine.dll 0x101f1571); its unload task
-- surfaces it before it sets off (0x102101bd), and ends at once with nothing
-- aboard (0x10210129).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do
            for x = 100, w - 100, 8 do
                if not site and depth(x, z) > 10 and depth(x + 40, z) > 10 and depth(x + 20, z) > 10 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        local carrier = CreateUnitHPR('ues0401', 'ARMY_1', sx, GetSurfaceHeight(sx, sz), sz, 0, 0, 0)
        local function dive()
            if carrier:GetCurrentLayer() == 'Sub' then return true end
            IssueDive({ carrier })
            for _ = 1, 200 do
                if carrier:GetCurrentLayer() == 'Sub' then return true end
                WaitTicks(1)
            end
        end
        check(dive(), 'the Atlantis dives')

        -- Load: three interceptors ordered aboard.
        local planes, stored_layers = {}, {}
        for i = 1, 3 do
            local p = CreateUnitHPR('uea0102', 'ARMY_1', sx + 10 + i * 3, GetSurfaceHeight(sx, sz) + 15, sz + 6, 0, 0, 0)
            p.OnAddToStorage = function(self, c) table.insert(stored_layers, c:GetCurrentLayer()) end
            table.insert(planes, p)
        end
        WaitTicks(5)
        IssueTransportLoad(planes, carrier)
        local surfacing = false
        for _ = 1, 300 do
            WaitTicks(1)
            surfacing = surfacing or carrier:IsUnitState('MovingUp')
            if table.getn(carrier:GetCargo()) == 3 then break end
        end
        note('stored with the carrier in: ' .. table.concat(stored_layers, ' '))
        check(surfacing, 'it surfaces for the load')
        check(table.getn(carrier:GetCargo()) == 3, 'all three stored, cargo ' .. table.getn(carrier:GetCargo()))
        local under = false
        for _, l in stored_layers do if l == 'Sub' then under = true end end
        check(not under, 'none goes in while it is under the water')

        -- Unload: it surfaces where it is, then sets off.
        check(dive(), 'the Atlantis dives again, loaded')
        local start = carrier:GetPosition()
        IssueTransportUnload({ carrier }, { sx + 40, GetSurfaceHeight(sx + 40, sz), sz })
        local moved_under = 0
        for _ = 1, 600 do
            WaitTicks(1)
            local p = carrier:GetPosition()
            if carrier:GetCurrentLayer() == 'Sub' then moved_under = math.max(moved_under, VDist2(p[1], p[3], start[1], start[3])) end
            if table.getn(carrier:GetCargo()) == 0 then break end
        end
        note(string.format('moved %.2f before it was up', moved_under))
        check(carrier:GetCurrentLayer() == 'Water', 'surfaced to unload: ' .. carrier:GetCurrentLayer())
        check(moved_under < 0.5, 'it surfaces before it sets off')
        check(table.getn(carrier:GetCargo()) == 0, 'unloaded')

        -- Nothing aboard: the unload order ends at once, under the water.
        check(dive(), 'the Atlantis dives, empty')
        local at = carrier:GetPosition()
        IssueTransportUnload({ carrier }, { sx, GetSurfaceHeight(sx, sz), sz })
        WaitTicks(3)
        local p = carrier:GetPosition()
        check(table.getn(carrier:GetCommandQueue()) == 0, 'an empty unload order ends at once')
        check(carrier:GetCurrentLayer() == 'Sub' and not carrier:IsUnitState('MovingUp'), 'and the carrier stays under: ' .. carrier:GetCurrentLayer())
        check(VDist2(p[1], p[3], at[1], at[3]) < 0.5, 'without moving')
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
