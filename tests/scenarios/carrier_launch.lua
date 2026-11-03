-- Scenario test (tests/scenarios.rs): an Atlantis launches its aircraft
--, as measured in the real game (dev/probes/maps/probe_launch).
-- A stored plane given a move order stays aboard. On an unload order the
-- carrier stays where it is, its aircraft are ordered to guard the unload
-- point, and from two ticks after the order one leaves every second tick,
-- in id order, through the next launch point (`Launchpoint` bones, else
-- the attach points, in skeleton order, the index advanced before use): at
-- the bone, facing along it, not at the hull centre. The order ends four
-- ticks after the last one leaves.
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
        WaitTicks(20)
        -- Its launch points: the attach bones, skeleton order (no
        -- `Launchpoint` bones on the Atlantis).
        local points = {}
        for i = 0, carrier:GetBoneCount() - 1 do
            local name = carrier:GetBoneName(i)
            if string.find(string.lower(name), 'launchpoint') then table.insert(points, i) end
        end
        if table.getn(points) == 0 then
            for i = 0, carrier:GetBoneCount() - 1 do
                if string.find(string.lower(carrier:GetBoneName(i)), 'attachpoint') then table.insert(points, i) end
            end
        end
        check(table.getn(points) == 8, 'eight attach points: ' .. table.getn(points))
        local planes = {}
        for i = 1, 4 do
            local p = CreateUnitHPR('uea0102', 'ARMY_1', sx + 10 + i * 3, GetSurfaceHeight(sx, sz) + 15, sz + 6, 0, 0, 0)
            carrier:AddUnitToStorage(p)
            table.insert(planes, p)
        end
        check(table.getn(carrier:GetCargo()) == 4, 'four stored')
        IssueMove({ planes[1] }, { sx + 30, GetSurfaceHeight(sx, sz) + 20, sz })
        WaitTicks(20)
        check(table.getn(carrier:GetCargo()) == 4, 'a stored plane ordered to move stays aboard: cargo ' .. table.getn(carrier:GetCargo()))
        local start = carrier:GetPosition()
        local target = { sx + 60, GetSurfaceHeight(sx + 60, sz), sz }
        IssueTransportUnload({ carrier }, target)
        -- Each tick: who left, and where they were then.
        local left, left_tick, at, tick, ended = {}, {}, {}, 0, nil
        for _ = 1, 20 do
            WaitTicks(1)
            tick = tick + 1
            for i, p in planes do
                if not left[i] and not p:IsUnitState('Attached') then
                    left[i] = true
                    left_tick[i] = tick
                    at[i] = p:GetPosition()
                end
            end
            if not ended and table.getn(carrier:GetCommandQueue()) == 0 then ended = tick end
        end
        check(table.getn(carrier:GetCargo()) == 0, 'all launched')
        check(ended == 13, 'the order ends four ticks after the last leaves: ' .. tostring(ended))
        local p = carrier:GetPosition()
        check(VDist2(p[1], p[3], start[1], start[3]) < 0.5, 'the carrier launches where it is')
        for i = 1, 4 do
            if left[i] then
                -- In id order, every second tick from the third, from the
                -- second point on.
                check(left_tick[i] == 1 + 2 * i, string.format('plane %d left on tick %d, not %d', i, left_tick[i], 1 + 2 * i))
                local bone = carrier:GetPosition(points[math.mod(i, table.getn(points)) + 1])
                local d = VDist3(at[i], bone)
                note(string.format('plane %d at %.2f %.2f %.2f, bone %.2f %.2f %.2f, hull %.2f %.2f %.2f', i, at[i][1], at[i][2], at[i][3], bone[1], bone[2], bone[3], p[1], p[2], p[3]))
                check(d < 0.05, string.format('plane %d leaves at its launch point (off by %.2f)', i, d))
            end
        end
        -- The Atlantis' points run down its deck (Attachpoint02 ... 08 in
        -- skeleton order, in pairs): the launches walk aft, off the hull
        -- centre, rather than all popping out of one spot.
        if at[1] and at[3] then
            check(VDist3(at[1], at[3]) > 1, string.format('launches spread down the deck: %.2f apart', VDist3(at[1], at[3])))
            check(VDist3(at[3], p) > 1, 'the third leaves away from the hull centre')
        end
        -- They guard the unload point.
        local q = planes[1]:GetCommandQueue()
        check(table.getn(q) == 1, 'the launched aircraft have one order: ' .. table.getn(q))
        WaitTicks(100)
        for i, pl in planes do
            local pp = pl:GetPosition()
            check(VDist2(pp[1], pp[3], target[1], target[3]) < 30, string.format('plane %d flew to the point: %.1f away', i, VDist2(pp[1], pp[3], target[1], target[3])))
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
