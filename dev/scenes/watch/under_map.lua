-- Dev: watches a running game for units left behind: live units more than 2
-- below the ground, and dead units still not destroyed 30 s after they
-- died (FAF's death threads end in Destroy). Each is logged once with its
-- blueprint, layer, states and height against the ground. Use with an AI game:
--   cargo run --release -- --headless --session --ticks 20000 --speed max --script dev/scenes/watch/under_map.lua
-- Every unit seen alive, and the tick it was first seen dead.
local known, dead_at = {}, {}
local function describe(u)
    local p = u:GetPosition()
    local g = GetTerrainHeight(p[1], p[3])
    local states = {}
    for _, s in { 'Attached', 'BeingBuilt', 'Moving', 'Building', 'Upgrading', 'Busy', 'Immobile' } do
        if u:IsUnitState(s) then table.insert(states, s) end
    end
    return string.format('%s (%s) army %d layer %s at %.1f, %.1f: %.1f above ground, fraction %.2f, states %s',
        u.UnitId, u:GetEntityId(), u.Army, u.Layer or '?', p[1], p[3], p[2] - g, u:GetFractionComplete(), table.concat(states, ','))
end
ForkThread(function()
    local seen = {}
    while true do
        WaitTicks(20)
        local now = GetGameTick()
        for u in known do
            if IsDestroyed(u) then
                known[u], dead_at[u] = nil, nil
            elseif u.Dead then
                dead_at[u] = dead_at[u] or now
                if now - dead_at[u] > 300 and not seen[u] then
                    seen[u] = true
                    LOG(string.format('LINGER tick %d dead since %d: %s', now, dead_at[u], describe(u)))
                end
            end
        end
        for _, brain in ArmyBrains do
            for _, u in brain:GetListOfUnits(categories.ALLUNITS - categories.STRUCTURE, false) or {} do
                known[u] = true
                if not u.Dead and not seen[u] then
                    local p = u:GetPosition()
                    if p[2] < GetTerrainHeight(p[1], p[3]) - 2 then
                        seen[u] = true
                        LOG(string.format('UNDERMAP tick %d %s', now, describe(u)))
                    end
                end
            end
        end
    end
end)
