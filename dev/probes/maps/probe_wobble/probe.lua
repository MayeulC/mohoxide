-- Differential probe: units standing still, and stopping after a
-- short move, logged every tick ("PROBE ..." lines, the same in both engines):
-- position and orientation quaternion, to see which wobble on the spot and how.
--   real game (Windows): dev/probes/run_faf.ps1 probe_wobble 60
--   ours: cargo run -- --headless --no-ai --ticks 200 --script dev/probes/maps/probe_wobble/probe.lua
-- Hover units carry moho's hover wobble (`Physics.WobbleFactor` /
-- `WobbleSpeed`); land units shouldn't move at all. On Seton's flat area A,
-- facing east. 'mv' units are ordered 8 east at tick 0 and stop there.
local TICKS = 150
local armies = ListArmies()
local function f5(n) return string.format('%.5f', n) end

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local all = {}
local E = math.pi / 2
local start = 0
local function spawn(tag, bp, x, z)
    local u = CreateUnitHPR(bp, armies[1], x, GetTerrainHeight(x, z), z, 0, E, 0)
    u:SetFireState(1)
    table.insert(all, { tag = tag, u = u, x = x, z = z })
    return u
end

-- Seraphim and, for comparison, the other factions' like units.
local ids = {
    { 'xsl0105', 'sera_t1eng' }, { 'xsl0208', 'sera_t2eng' }, { 'xsl0309', 'sera_t3eng' },
    { 'xsl0203', 'sera_hovertank' }, { 'xsl0307', 'sera_shield' }, { 'xsl0201', 'sera_t1tank' },
    { 'xsl0101', 'sera_scout' }, { 'xsl0001', 'sera_acu' },
    { 'ual0105', 'aeon_t1eng' }, { 'ual0201', 'aeon_t1tank' }, { 'uel0203', 'uef_hovertank' }, { 'uel0105', 'uef_t1eng' },
}
local movers = {}
for i, e in ids do
    spawn(e[2], e[1], 600 + i * 6, 90)
    table.insert(movers, spawn('mv_' .. e[2], e[1], 600 + i * 6, 110))
end

ForkThread(function()
    WaitTicks(10)
    start = GetGameTick()
    for _, u in movers do
        local p = u:GetPosition()
        IssueMove({ u }, { p[1], 0, p[3] + 8 })
    end
    for t = 0, TICKS do
        for _, e in all do
            local u = e.u
            local p = u:GetPosition()
            local q = u:GetOrientation()
            LOG('PROBE ' .. e.tag .. ' ' .. t .. ' pos ' .. f5(p[1]) .. ' ' .. f5(p[2]) .. ' ' .. f5(p[3])
                .. ' q ' .. f5(q[1]) .. ' ' .. f5(q[2]) .. ' ' .. f5(q[3]) .. ' ' .. f5(q[4]))
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
end)
