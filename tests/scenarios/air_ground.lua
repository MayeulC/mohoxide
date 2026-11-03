-- Scenario test (tests/scenarios.rs): `BankForward` and
-- `GroundCollisionOffset`. A Cybran T1 gunship (BankForward) sent straight
-- ahead pitches nose down into its acceleration; a UEF T2 gunship (not)
-- stays level. Flown down into the ground, a UEF T1 transport (offset 0)
-- is stopped as its collision box touches; a T2 transport given offset 1
-- sinks about that far first. (FAF's aircraft pair the offset with their
-- `SizeSphere`, so a gunship stops at its position.)
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
-- How far under the terrain a unit's lowest collision-box corner is (its
-- blueprint box: Size*, CollisionOffsetY; none of these set X / Z).
local function box_depth(u)
    local bp = u:GetBlueprint()
    local q = u:GetOrientation()
    local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
    local px, py, pz = u:GetPositionXYZ()
    local deepest = -100
    for _, sx in {-1, 1} do for _, sy in {-1, 1} do for _, sz in {-1, 1} do
        local vx, vy, vz = sx * bp.SizeX / 2, (bp.CollisionOffsetY or 0) + bp.SizeY / 2 + sy * bp.SizeY / 2, sz * bp.SizeZ / 2
        -- v + 2w (q x v) + 2 q x (q x v)
        local cx, cy, cz = qy * vz - qz * vy, qz * vx - qx * vz, qx * vy - qy * vx
        local x = px + vx + 2 * qw * cx + 2 * (qy * cz - qz * cy)
        local y = py + vy + 2 * qw * cy + 2 * (qz * cx - qx * cz)
        local z = pz + vz + 2 * qw * cz + 2 * (qx * cy - qy * cx)
        deepest = math.max(deepest, GetTerrainHeight(x, z) - y)
    end end end
    return deepest
end
-- The y of the unit's forward (local +Z) axis.
local function nose_y(u)
    local q = u:GetOrientation()
    local x, y, z, w = q[1], q[2], q[3], q[4]
    return 2 * (y * z - w * x)
end
ForkThread(function()
    WaitTicks(2)
    -- Pitch: both at their height, then sent north.
    local jester = CreateUnitHPR('xra0105', 'ARMY_1', 300, GetTerrainHeight(300, 300) + 10, 300, 0, 0, 0)
    local gunship = CreateUnitHPR('uea0203', 'ARMY_1', 320, GetTerrainHeight(320, 300) + 10, 300, 0, 0, 0)
    WaitTicks(20)
    IssueMove({jester}, Vector(300, 0, 500))
    IssueMove({gunship}, Vector(320, 0, 500))
    local jester_pitch, gunship_pitch = 0, 0
    for t = 1, 30 do
        WaitTicks(1)
        jester_pitch = math.min(jester_pitch, nose_y(jester))
        gunship_pitch = math.min(gunship_pitch, nose_y(gunship))
    end
    -- Ground: told to fly 3 under it (`SetElevation`), so they come down
    -- into it. How far under the terrain each one's lowest point goes: the
    -- transports' box corners; the gunship's position (its collision is a
    -- 1.6 sphere there, and offset 1.6 cancels the radius).
    __blueprints['uea0104'].Physics.GroundCollisionOffset = 1
    local spots = {
        {id = 'uea0107', x = 350, z = 850, box = true},
        {id = 'uea0104', x = 300, z = 900, box = true},
        {id = 'uea0203', x = 350, z = 900},
    }
    for _, s in spots do
        s.unit = CreateUnitHPR(s.id, 'ARMY_1', s.x, GetTerrainHeight(s.x, s.z) + 2, s.z, 0, 0, 0)
        s.depth = -10
    end
    WaitTicks(1)
    for _, s in spots do
        s.unit:SetElevation(-3)
        IssueMove({s.unit}, Vector(s.x, 0, s.z + 2))
    end
    for t = 1, 40 do
        WaitTicks(1)
        for _, s in spots do
            local x, y, z = s.unit:GetPositionXYZ()
            s.depth = math.max(s.depth, s.box and box_depth(s.unit) or GetTerrainHeight(x, z) - y)
        end
    end
    local transport_depth, offset_depth, sunk_depth = spots[1].depth, spots[2].depth, spots[3].depth
    note(string.format('lowest nose y: jester (BankForward) %.4f, gunship %.4f; deepest under the terrain: T1 transport %.3f, T2 transport (offset 1) %.3f, gunship %.3f',
        jester_pitch, gunship_pitch, transport_depth, offset_depth, sunk_depth))
    check(jester_pitch < -0.002, 'a BankForward gunship pitches into its acceleration')
    check(gunship_pitch > -0.0005, 'a gunship without BankForward stays level ahead')
    check(transport_depth < 0.1, 'the ground stops a transport at its collision box')
    check(offset_depth > 0.7 and offset_depth < 1.6, 'with GroundCollisionOffset 1 its box goes about 1 under before the ground pushes back')
    check(sunk_depth < 0.1, 'the ground stops a gunship at its position')
    ScenarioResult.done = true
end)
