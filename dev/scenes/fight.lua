-- Development scenario: two UEF Strikers (army 1) face two Cybran Mantises
-- (army 2) in the middle of the map, inside each other's weapon range.
-- Run: cargo run -- --headless --ticks 300 --script dev/scenes/fight.lua
local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local function spawn(bp, army, x, z, heading)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading, 0)
    LOG('dev/fight: ', bp, ' army ', army, ' entity ', u:GetEntityId())
    return u
end
local units = {
    spawn('uel0201', 1, cx - 8, cz - 2, math.pi / 2),
    spawn('uel0201', 1, cx - 8, cz + 2, math.pi / 2),
    spawn('url0107', 2, cx + 8, cz - 2, -math.pi / 2),
    spawn('url0107', 2, cx + 8, cz + 2, -math.pi / 2),
}

-- Log their health every second (log only; no sim effect).
ForkThread(function()
    for second = 1, 15 do
        WaitSeconds(1)
        local line = ''
        for _, u in units do
            line = line .. string.format(' %s[%s]=%s', u.UnitId or '?', u.Army or '?',
                u:BeenDestroyed() and 'gone' or (u.Dead and 'dead' or tostring(math.floor(u:GetHealth()))))
        end
        LOG('dev/fight t=', second, 's:', line)
    end
end)
