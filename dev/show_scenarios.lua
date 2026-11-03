-- Appended to each scenario by dev/show_scenarios.sh / .ps1, after a line
-- setting SHOW = { name = ..., zoom = ..., hold = ... }. Not a script of its own.

-- The camera follows every unit on the map (the armies have no commanders,
-- so they're all the scenario's), zoomed to fit them; re-aimed every couple
-- of seconds as units move, appear and die.
ForkThread(function()
    local camera = import('/lua/simcamera.lua').SimCamera('WorldCamera')
    local width, height = GetMapSize()
    local last_ids, last_zoom = '', 0
    while true do
        WaitTicks(1)
        local units = {}
        local x0, z0, x1, z1 = math.huge, math.huge, -math.huge, -math.huge
        for _, u in GetUnitsInRect(Rect(0, 0, width, height)) or {} do
            if not u:IsDead() and not u:IsUnitState('Attached') then
                local x, _, z = unpack(u:GetPosition())
                x0, z0, x1, z1 = math.min(x0, x), math.min(z0, z), math.max(x1, x), math.max(z1, z)
                table.insert(units, u)
            end
        end
        if table.getn(units) > 0 then
            local ids = ''
            for _, u in units do ids = ids .. u:GetEntityId() .. ',' end
            local zoom = math.min(math.max(math.max(x1 - x0, z1 - z0) * 1.4 + 40, SHOW.zoom), 600)
            if ids ~= last_ids or math.abs(zoom - last_zoom) > last_zoom * 0.15 then
                camera:TrackEntities(units, zoom, 1)
                last_ids, last_zoom = ids, zoom
            end
        end
        WaitTicks(19)
    end
end)
-- Title, then end the game once the scenario is done.
ForkThread(function()
    WaitTicks(1)
    PrintText(SHOW.name, 24, 'ffffffff', 6, 'center')
    while not ScenarioResult.done do WaitTicks(1) end
    local f = ScenarioResult.failures
    PrintText(table.getn(f) == 0 and (SHOW.name .. ': passed') or (SHOW.name .. ': ' .. table.getn(f) .. ' failed'), 20,
        table.getn(f) == 0 and 'ff40ff40' or 'ffff4040', 3, 'center')
    WaitTicks(SHOW.hold)
    EndGame()
end)
