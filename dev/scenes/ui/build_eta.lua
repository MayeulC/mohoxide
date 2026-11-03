-- Waypoint ETAs on a shared build: six UEF engineers on one air
-- factory. Three get the build order together (one command), two are sent
-- to help build it once it's up (a repair order on it) and one assists
-- the first. Energy is short (a little stored, then a power generator's
-- trickle), so the build stalls. Engineer ids are logged as BUILDETA lines
-- (for --select), with the energy and the structure's progress.
-- Run: cargo run -- --no-ai --script dev/scenes/ui/build_eta.lua --camera 410,600,20 --select 5391,5392,5393,5394,5395,5396
-- (the ids on the default map, Seton's Clutch)
local function spawn(bp, x, z)
    return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
end
local brain = ArmyBrains[1]
brain:__engine_SetStorage('MASS', 5000)
brain:GiveResource('MASS', 5000)
-- Energy for a fifth of the factory, then a stall on a power generator's trickle.
brain:__engine_SetStorage('ENERGY', 500)
brain:GiveResource('ENERGY', 500)
spawn('ueb1101', 396, 606) -- 20 energy a second: the build runs at a fraction
local engineers = {}
for i = 1, 6 do
    engineers[i] = spawn('uel0105', 398 + i * 3, 592)
end
local ids = {}
for i, e in engineers do ids[i] = e:GetEntityId() end
LOG('BUILDETA engineers ' .. table.concat(ids, ','))
local site = {410.5, GetTerrainHeight(410.5, 602.5), 602.5}
ForkThread(function()
    WaitTicks(5)
    IssueBuildAllMobile({engineers[1], engineers[2], engineers[3]}, site, 'ueb0102', {})
    -- Once the structure is up, two more engineers are sent to help build
    -- it (right-clicking it: a repair order) and the last assists the first.
    local structure
    while not structure do
        WaitTicks(1)
        for i = 1, 3 do structure = structure or engineers[i]:GetFocusUnit() end
    end
    IssueRepair({engineers[4], engineers[5]}, structure)
    IssueGuard({engineers[6]}, engineers[1])
    while true do
        WaitTicks(10)
        local f = engineers[1]:GetFocusUnit() or engineers[4]:GetFocusUnit()
        LOG(string.format('BUILDETA tick %d energy %.0f progress %s', GetGameTick(), brain:GetEconomyStored('ENERGY'),
            f and string.format('%.3f', f:GetFractionComplete()) or '-'))
    end
end)
