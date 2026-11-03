-- Demo: one human ACU, a T3 interceptor to fly, and a ring of
-- enemy structures about 40 units out - all beyond the ACU's 26-unit
-- vision, so nothing is scouted until the interceptor flies over it. Fly a
-- pass over the ring: each structure is drawn as it is while seen, then,
-- out of sight again, stays as its own strategic icon greyed at its
-- last-known spot (the remembered blip) instead of disappearing. The
-- un-explored map around them stays dark/grey while the icons hold.
-- Run: cargo run -- --no-ai --player 1 --armies 2 --teams 0 --script dev/scenes/play/fog_of_war.lua --camera 430,600,120
local b = ArmyBrains[1]
b:__engine_SetStorage('MASS', 50000)
b:__engine_SetStorage('ENERGY', 500000)
b:GiveResource('MASS', 50000)
b:GiveResource('ENERGY', 500000)

local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading, 0)
end

local acu = spawn('uel0001', 'ARMY_1', 430, 600, 0)

-- The T3 interceptor lifts off next to the ACU; select it (click it, or
-- drag round the two starting units) and fly.
local jet = spawn('uea0303', 'ARMY_1', 436, 596, 0)
LOG('demo: ACU ', acu:GetEntityId(), ', T3 interceptor ', jet:GetEntityId())

-- A ring of enemy structures ~40 units out: factories, a T2 power plant, a
-- T2 extractor, point defences and a T3 omni.
local ring = {
    { 'UEB0101', 430, 556 }, -- land factory, north
    { 'UEB0102', 394, 570 }, -- air factory, west
    { 'UEB0103', 466, 570 }, -- naval factory, east
    { 'UEB1201', 396, 626 }, -- T2 power generator, south-west
    { 'UEB1202', 464, 626 }, -- T2 mass extractor, south-east
    { 'UEB2101', 408, 556 }, -- point defence, north-west flank
    { 'UEB2101', 452, 556 }, -- point defence, north-east flank
    { 'UEB3104', 430, 640 }, -- T3 omni sensor, south
}
for _, s in ipairs(ring) do
    spawn(s[1], 'ARMY_2', s[2], s[3], 0)
    LOG('demo: enemy structure ', s[1], ' at ', s[2], ', ', s[3])
end
