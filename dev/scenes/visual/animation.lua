-- Development scene for model animation: tanks whose targets sit off to
-- the side (turrets yaw ~90 degrees), walkers on the move (walk clips), and
-- radars (spinning rotators), on the land patch of scmp_009.
-- Run: cargo run -- --script dev/scenes/visual/animation.lua --camera 420,600,45
local function spawn(bp, army, x, z, heading)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, heading or 0, 0)
end

-- Tanks face north (+z); their enemies are due east / west of them.
spawn('uel0201', 1, 405, 595)
spawn('uel0201', 1, 405, 605)
spawn('ual0201', 2, 420, 595)
spawn('ual0201', 2, 420, 605)

-- Walkers crossing the scene (out of each other's range).
local hunter = spawn('url0106', 1, 400, 625, math.pi / 2)
local mech = spawn('uel0106', 2, 440, 575, -math.pi / 2)
IssueMove({hunter}, {440, GetTerrainHeight(440, 625), 625})
IssueMove({mech}, {400, GetTerrainHeight(400, 575), 575})

-- Radars spin their dishes.
spawn('ueb3101', 1, 395, 580)
spawn('uab3101', 2, 445, 620)
