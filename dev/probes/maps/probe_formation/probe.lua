-- Differential probe: how tightly ground units pack (footprints become
-- their pathing class's, and our avoidance radius comes from them).
--   real game (Windows): dev/probes/run_faf.ps1 probe_formation 90
--   ours: cargo run -- --headless --no-ai --ticks 460 --script dev/probes/maps/probe_formation/probe.lua
-- 1. The formation scenario: 12 uel0201 and 4 uel0101 form-moved with
--    AttackFormation (tests/scenarios/formation.lua).
-- 2. A clump: 8 uel0201 all moved to one point.
-- After 40 s, every unit's position.
local units, clump = {}, {}
for i = 0, 11 do
    local x, z = 380 + math.mod(i, 4) * 3, 560 + math.floor(i / 4) * 3
    table.insert(units, CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0))
end
for i = 0, 3 do table.insert(units, CreateUnitHPR('uel0101', 'ARMY_1', 380 + i * 3, GetTerrainHeight(380 + i * 3, 572), 572, 0, 0, 0)) end
for i = 0, 7 do
    local x, z = 300 + math.mod(i, 4) * 4, 640 + math.floor(i / 4) * 4
    table.insert(clump, CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0))
end

local function f2(n) return string.format('%.2f', n) end
local function report(tag, list)
    local minD = 1e9
    for i, u in list do
        local p = u:GetPosition()
        LOG('PROBE ' .. tag .. ' ' .. i .. ' ' .. f2(p[1]) .. ' ' .. f2(p[3]))
        for j = i + 1, table.getn(list) do
            local q = list[j]:GetPosition()
            minD = math.min(minD, VDist2(p[1], p[3], q[1], q[3]))
        end
    end
    LOG('PROBE ' .. tag .. ' closest ' .. f2(minD))
end

ForkThread(function()
    WaitTicks(2)
    IssueFormMove(units, { 420, 0, 600 }, 'AttackFormation', 90)
    IssueMove(clump, { 320, 0, 660 })
    WaitTicks(400)
    report('form', units)
    report('clump', clump)
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
