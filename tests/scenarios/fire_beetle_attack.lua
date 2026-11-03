-- Scenario test (tests/scenarios.rs): FAF's Fire Beetle
-- (xrl0302) enables its kamikaze weapon only while `IsUnitState('Attacking')`
-- (its `TrackTargetThread`, every 6 ticks), so it detonates on an attack
-- order and never at an enemy it merely sees. The real game's
-- (dev/probes/maps/probe_auditgaps, lane g5): no order, enemy 4.5 away:
-- never fires; `IssueAttack` at 4.5: fires 6 ticks after the order;
-- `IssueAttack` from 20: walks in and fires; on hold fire, or moved onto
-- the target: never fires.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        -- Made from a thread, as the game makes units after the start.
        local cases = { { 'none', 4.5 }, { 'attack', 4.5 }, { 'far', 20 }, { 'hold', 4.5 }, { 'move', 20 } }
        -- Dry, flat sites (the weapon only targets above the water).
        local function dry(x, z)
            for dz = -25, 5, 5 do
                local h = GetTerrainHeight(x, z + dz)
                if GetSurfaceHeight(x, z + dz) > h + 0.01 or math.abs(h - GetTerrainHeight(x, z)) > 3 then return false end
            end
            return true
        end
        local sites = {}
        local w, h = GetMapSize()
        for z = 40, h - 40, 16 do
            for x = 40, w - 40, 32 do
                if table.getn(sites) < table.getn(cases) and dry(x, z) then table.insert(sites, { x, z }) end
            end
        end
        check(table.getn(sites) == table.getn(cases), 'found dry sites')
        if table.getn(sites) < table.getn(cases) then return end
        local lanes = {}
        for i, c in cases do
            local x, z = sites[i][1], sites[i][2]
            local tank = CreateUnitHPR('uel0201', 'ARMY_2', x, GetTerrainHeight(x, z), z, 0, 0, 0)
            tank:SetFireState(1)
            tank:SetMaxHealth(30000) tank:SetHealth(nil, 30000)
            local beetle = CreateUnitHPR('xrl0302', 'ARMY_1', x, GetTerrainHeight(x, z - c[2]), z - c[2], 0, 0, 0)
            if c[1] == 'hold' then beetle:SetFireState(1) end
            local lane = { kind = c[1], tank = tank, beetle = beetle }
            local on = tank.OnDamage
            tank.OnDamage = function(self, inst, amount, vec, kind)
                on(self, inst, amount, vec, kind)
                if kind == 'EMP' and not lane.hit then lane.hit = GetGameTick() end
            end
            table.insert(lanes, lane)
        end
        WaitTicks(5)
        local ordered = GetGameTick()
        for _, l in lanes do
            if l.kind == 'move' then IssueMove({ l.beetle }, l.tank:GetPosition())
            elseif l.kind ~= 'none' then IssueAttack({ l.beetle }, l.tank) end
        end
        WaitTicks(100)
        for _, l in lanes do
            note(string.format('%s: %s', l.kind, l.hit and ('fired ' .. (l.hit - ordered) .. ' ticks after the order') or 'never fired'))
            if l.kind == 'attack' then
                check(l.hit and l.hit - ordered <= 10, 'an attack order at 4.5 fires within 10 ticks')
            elseif l.kind == 'far' then
                check(l.hit ~= nil, 'an attack order from 20 walks in and fires')
            else
                check(not l.hit, l.kind .. ': never fires')
            end
        end
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
