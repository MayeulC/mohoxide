-- Differential probe: how units ease into a stop. Land, hover and
-- naval units each make a short (2), medium (8) and long (30) straight move
-- along +z from rest, facing it; position and orientation logged every tick
-- ("PROBE ..." lines, the same in both engines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_arrive 90
--   ours: cargo run -- --headless --no-ai --ticks 260 --script dev/probes/maps/probe_arrive/probe.lua
-- Real-game orders land 3 ticks after IssueMove (ours 0).
local TICKS = 220
local armies = ListArmies()
local function f5(n) return string.format('%.5f', n) end

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local movers = {}
-- On the footprint's grid (cell centres for odd sizes, corners for even),
-- where the real game snaps a move's goal, so both engines steer for the
-- same point.
local function snap(v, size) return math.floor(v - size / 2 + 0.5) + size / 2 end
local function spawn(tag, bp, x, z, d, water)
    local fp = __blueprints[bp].Footprint or {}
    x, z = snap(x, fp.SizeX or 1), snap(z, fp.SizeZ or 1)
    local y = water and GetSurfaceHeight(x, z) or GetTerrainHeight(x, z)
    local u = CreateUnitHPR(bp, armies[1], x, y, z, 0, 0, 0)
    u:SetFireState(1)
    table.insert(movers, { tag = tag, u = u, x = x, z = z, d = d })
    LOG('PROBE spawn ' .. tag .. ' ' .. f5(x) .. ' ' .. f5(z) .. ' ground ' .. f5(GetTerrainHeight(x, z)) .. ' surface ' .. f5(GetSurfaceHeight(x, z)))
end

local land = {
    { 'uel0201', 'uef_t1tank' }, { 'url0107', 'cyb_t1bot' }, { 'uel0105', 'uef_t1eng' }, { 'uel0303', 'uef_t3bot' },
    { 'xsl0105', 'sera_t1eng' }, { 'ual0201', 'aeon_t1hover' }, { 'uel0203', 'uef_hovertank' },
}
local naval = { { 'ues0103', 'uef_frigate' }, { 'uas0103', 'aeon_frigate' }, { 'ues0201', 'uef_destroyer' } }
local dists = { 2, 8, 30 }
for i, e in land do
    for j, d in dists do
        spawn(e[2] .. '_' .. d, e[1], 600 + i * 7, 70 + (j - 1) * 14, d)
    end
end
for i, e in naval do
    for j, d in dists do
        spawn(e[2] .. '_' .. d, e[1], 110 + ((i - 1) * 3 + j) * 14, 50, d, true)
    end
end

ForkThread(function()
    WaitTicks(20)
    for _, m in movers do
        local p = m.u:GetPosition()
        IssueMove({ m.u }, { p[1], 0, p[3] + m.d })
    end
    for t = 0, TICKS do
        for _, m in movers do
            local u = m.u
            if not u:BeenDestroyed() then
                local p = u:GetPosition()
                local q = u:GetOrientation()
                LOG('PROBE ' .. m.tag .. ' ' .. t .. ' pos ' .. f5(p[1]) .. ' ' .. f5(p[2]) .. ' ' .. f5(p[3])
                    .. ' q ' .. f5(q[1]) .. ' ' .. f5(q[2]) .. ' ' .. f5(q[3]) .. ' ' .. f5(q[4]))
            end
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
end)
