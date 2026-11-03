-- Differential probe: when an army first sees an enemy that appears in its
-- vision, logged in a format both engines print identically.
--   real game (Windows): dev/probes/run_faf.ps1 probe_sight 60
--   ours: cargo run -- --headless --no-ai --ticks 120 --script dev/probes/maps/probe_sight/probe.lua
-- 10 still uel0201 (vision 20) per army on Seton's area B, 40 apart. Each
-- observer k has an enemy tank parked out of sight, warped to 10 in front of
-- it on tick 60 + k (every phase of a 10-tick cycle; the start units' vision
-- has gone by then). Targets hold fire. Each tick from 58:
-- per observer, whether its army sees its target now ('S' / '.') and
-- whether the observer's weapon has it ('T' / '.').
local armies = ListArmies()

local function sweep(keep)
    for _, brain in ArmyBrains do
        for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
            if not keep[u:GetEntityId()] and not u:BeenDestroyed() then u:Destroy() end
        end
    end
end
sweep({})

local ids = {}
local function spawn(x, z, army)
    local u = CreateUnitHPR('uel0201', army, x, GetTerrainHeight(x, z), z, 0, math.pi / 2, 0)
    ids[u:GetEntityId()] = true
    return u
end
-- Observers: army 1 rows z 800 / 840, army 2 rows z 880 / 920; x 210..370.
local pairs = {}
for a = 1, 2 do
    for k = 0, 9 do
        local x, z = 210 + math.mod(k, 5) * 40, 800 + (a - 1) * 80 + math.floor(k / 5) * 40
        local obs = spawn(x, z, armies[a])
        local park = { 70 + k * 2, 524 + (a - 1) * 4 }
        local tgt = spawn(park[1], park[2], armies[3 - a])
        table.insert(pairs, { a = a, k = k, obs = obs, tgt = tgt, at = { x + 10, z } })
    end
end

local function seen(p)
    local army = p.obs:GetArmy()
    local ok, r = pcall(function() local b = p.tgt:GetBlip(army) return b and b:IsSeenNow(army) end)
    if not ok then return '?' end
    return r and 'S' or '.'
end
local function targeted(p)
    local ok, t = pcall(function() return p.obs:GetWeapon(1):GetCurrentTarget() end)
    if not ok or not t then return '.' end
    local ok2, id = pcall(function() return t:GetEntityId() end)
    return (ok2 and id == p.tgt:GetEntityId()) and 'T' or '.'
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    -- Targets hold fire (no shooting at the observers first).
    for _, p in pairs do p.tgt:SetFireState(1) end
    local start = GetGameTick()
    LOG('PROBE info start=' .. start)
    while GetGameTick() - start <= 100 do
        local t = GetGameTick() - start
        for _, p in pairs do
            if t == 60 + p.k then
                Warp(p.tgt, { p.at[1], GetTerrainHeight(p.at[1], p.at[2]), p.at[2] })
            end
        end
        if t >= 58 then
            local row = 'PROBE ' .. t .. ' game=' .. GetGameTick()
            for _, p in pairs do row = row .. ' ' .. p.a .. p.k .. '=' .. seen(p) .. targeted(p) end
            LOG(row)
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
