-- Differential probe: a recon blip's lifetime (docs/research/verifying-against-the-binary.md,
-- "Blip objects and lifetime"), logged in a format both engines print the same.
--   real game (Windows): dev/probes/run_faf.ps1 probe_blip 60
--   ours: cargo run -- --headless --no-ai --ticks 100 --script dev/probes/maps/probe_blip/probe.lua
-- Army 2 observers (uel0201, vision 20) each watch an army 1 target 10 east;
-- everything holds fire. At t=10 army 2's blip of each target is kept, given a
-- field and a destroy hook. Cases:
--   A tank leaves sight (t=20), comes back (t=50)
--   B tank destroyed in sight (t=30)
--   C tank leaves sight (t=20), destroyed out of sight (t=30), its observer
--     goes to where it was seen (t=50)
--   D structure, its observer leaves (t=20), destroyed unseen (t=30), observer
--     back (t=50)
--   E structure, its observer leaves (t=20), FlushIntelInRect over it (t=40)
-- Each tick from 9, per case: GetBlip(2) now ('-' nil, '=' the kept blip,
-- 'N' another), the kept blip's field ('f' still there, '.' gone, '?' error),
-- IsMaybeDead ('m' / '.' / '?'), and 'H' once its destroy hook has run.
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
local function spawn(bp, x, z, army)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, math.pi / 2, 0)
    ids[u:GetEntityId()] = true
    return u
end
local function warp(u, x, z)
    Warp(u, { x, GetTerrainHeight(x, z), z })
end

local cases = {}
for i, c in { { 'A', 'uel0201' }, { 'B', 'uel0201' }, { 'C', 'uel0201' }, { 'D', 'ueb2101' }, { 'E', 'ueb2101' } } do
    local x, z = 160 + i * 40, 820
    table.insert(cases, { name = c[1], x = x, z = z, obs = spawn('uel0201', x, z, armies[2]), tgt = spawn(c[2], x + 10, z, armies[1]) })
end
local by = {}
for _, c in cases do by[c.name] = c end

local function cell(c)
    local now = '?'
    local ok, b = pcall(function() return c.tgt:GetBlip(2) end)
    if ok then now = (b == nil and '-') or (rawequal(b, c.blip) and '=') or 'N' end
    local field = '?'
    local okf, f = pcall(function() return rawget(c.blip, 'Note') end)
    if okf then field = (f == 'kept' and 'f') or '.' end
    local dead = '?'
    local okd, d = pcall(function() return c.blip:IsMaybeDead(2) end)
    if okd then dead = d and 'm' or '.' end
    return now .. field .. dead .. (c.hooked and 'H' or '.')
end

ForkThread(function()
    WaitTicks(2)
    sweep(ids)
    for _, c in cases do
        if c.tgt.SetFireState then c.tgt:SetFireState(1) end
        c.obs:SetFireState(1)
    end
    local start = GetGameTick()
    LOG('PROBE info start=' .. start)
    while GetGameTick() - start <= 70 do
        local t = GetGameTick() - start
        if t == 10 then
            for _, c in cases do
                c.blip = c.tgt:GetBlip(2)
                if c.blip then
                    c.blip.Note = 'kept'
                    local name = c.name
                    c.blip:AddDestroyHook(function(b) by[name].hooked = true LOG('PROBE hook ' .. name .. ' t=' .. (GetGameTick() - start)) end)
                end
            end
            local blip = by.A.blip
            local class = blip and getmetatable(blip)
            LOG('PROBE info class=' .. tostring(class == import('/lua/sim/Blip.lua').Blip) .. ' kept=' .. (by.A.blip and 'A' or '') .. (by.B.blip and 'B' or '') .. (by.C.blip and 'C' or '') .. (by.D.blip and 'D' or '') .. (by.E.blip and 'E' or ''))
        elseif t == 20 then
            warp(by.A.tgt, 70, 524)
            by.C.seen = { by.C.tgt:GetPosition()[1], by.C.tgt:GetPosition()[3] }
            warp(by.C.tgt, 90, 524)
            warp(by.D.obs, 70, 600)
            warp(by.E.obs, 90, 600)
        elseif t == 30 then
            by.B.tgt:Destroy()
            by.C.tgt:Destroy()
            by.D.tgt:Destroy()
        elseif t == 40 then
            FlushIntelInRect(by.E.x + 5, by.E.z - 5, by.E.x + 15, by.E.z + 5)
        elseif t == 50 then
            warp(by.A.tgt, by.A.x + 10, by.A.z)
            warp(by.C.obs, by.C.seen[1] - 5, by.C.seen[2])
            warp(by.D.obs, by.D.x, by.D.z)
        end
        if t >= 9 and by.A.blip then
            local row = 'PROBE ' .. t
            for _, c in cases do row = row .. ' ' .. c.name .. '=' .. (c.blip and cell(c) or 'none') end
            LOG(row)
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
