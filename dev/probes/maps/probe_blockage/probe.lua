-- Differential probe: weapon target searches and terrain blockage,
-- logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_blockage 60
--   ours: cargo run -- --headless --no-ai --ticks 80 --script dev/probes/maps/probe_blockage/probe.lua
-- Idle uel0201 (no orders). On Seton's west plateau a tank (A, army 1) has
-- an enemy 12 east behind a ridge 3.7 above them both (B) and one 14 west
-- down the slope with nothing between (C): the real search skips blocked
-- candidates (MohoEngine.dll 0x101c5058), so A should take C although B is
-- nearer. The map's east twin (932, 492) repeats it, mirrored.
-- Every tick: each unit's weapon target (probe index, '-' none).
local TICKS = 60
local armies = ListArmies()

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local all, ids, index = {}, {}, {}
local function spawn(tag, x, z, yaw, army)
    local u = CreateUnitHPR('uel0201', army or armies[1], x, GetTerrainHeight(x, z), z, 0, yaw or 0, 0)
    table.insert(all, { tag = tag, u = u })
    ids[u:GetEntityId()] = true
    index[u:GetEntityId()] = table.getn(all)
    return u
end
local E, W = math.pi / 2, -math.pi / 2
spawn('A', 80, 532, E)
spawn('B', 92, 532, W, armies[2])
spawn('C', 66, 532, E, armies[2])
spawn('A2', 932, 492, W)
spawn('B2', 920, 492, E, armies[2])
spawn('C2', 946, 492, W, armies[2])

local function target_of(u)
    local ok, t = pcall(function() return u:GetWeapon(1):GetCurrentTarget() end)
    if not ok or not t then return '-' end
    local ok2, id = pcall(function() return t:GetEntityId() end)
    if not ok2 then return '?' end
    return tostring(index[id] or ('e' .. id))
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    for i, s in all do
        local x, y, z = s.u:GetPositionXYZ()
        LOG('PROBE info unit ' .. i .. ' ' .. s.tag .. ' ' .. string.format('%.2f %.2f %.2f', x, y, z))
    end
    local start = GetGameTick()
    while GetGameTick() - start <= TICKS do
        local t = GetGameTick() - start
        local row = 'PROBE ' .. t
        for i, s in all do
            row = row .. ' ' .. s.tag .. '=' .. ((s.u:BeenDestroyed() or s.u.Dead) and 'x' or target_of(s.u))
        end
        LOG(row)
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
