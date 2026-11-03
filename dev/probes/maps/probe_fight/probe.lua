-- Differential probe: the dev/scenes/fight.lua skirmish, logged per tick in a format
-- both engines print identically ("PROBE ..." lines). Uses engine APIs only,
-- so it runs on stock FA as well as on our engine with FAF's Lua.
--   real game: dev/probes/run_fa.sh probe_fight
--   ours:      cargo run -- --headless --ticks 160 --script dev/probes/maps/probe_fight/probe.lua
local TICKS = 150

local function fmt(n) return string.format('%.2f', n) end

local armies = ListArmies()
LOG('PROBE armies ', table.getn(armies), ' ', armies[1] or '-', ' ', armies[2] or '-')

local w, h = GetMapSize()
local cx, cz = w / 2, h / 2
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading, 0)
end
local units = {
    spawn('uel0201', armies[1], cx - 8, cz - 2, math.pi / 2),
    spawn('uel0201', armies[1], cx - 8, cz + 2, math.pi / 2),
    spawn('url0107', armies[2], cx + 8, cz - 2, -math.pi / 2),
    spawn('url0107', armies[2], cx + 8, cz + 2, -math.pi / 2),
}

ForkThread(function()
    local start = GetGameTick()
    for t = 0, TICKS do
        local line = 'PROBE t=' .. (GetGameTick() - start)
        for i, u in units do
            if u:BeenDestroyed() then
                line = line .. ' ' .. i .. ':gone'
            else
                local x, y, z = u:GetPositionXYZ()
                line = line .. ' ' .. i .. ':' .. math.floor(u:GetHealth()) .. '@' .. fmt(x) .. ',' .. fmt(z)
            end
        end
        LOG(line)
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
