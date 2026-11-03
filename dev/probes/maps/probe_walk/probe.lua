-- Differential probe: walk cycles against the walker's motion
-- (`CreateAnimator(unit, true)`), logged every tick in a format both engines
-- print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_walk 60
--   ours: cargo run -- --headless --no-ai --ticks 160 --script dev/probes/maps/probe_walk/probe.lua
-- Per unit and tick: the walk clip's time ('-' without a walk animator),
-- speed (per second), heading (degrees). Horizontal motion events as they come.
-- All on Seton's flat area A, facing east:
--   line: a Mantis (url0107) walks 40 straight east and stops.
--   behind: a Mantis ordered 20 straight behind it (turn, then walk).
--   spin: a Mantis ordered 2 behind it (mostly turning).
--   side: a Mantis ordered 6 to its left.
--   marine: a Mech Marine (uel0106) ordered 20 straight behind it.
local TICKS = 140
local armies = ListArmies()
local function f2(n) return string.format('%.2f', n) end
local function f3(n) return string.format('%.3f', n) end

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
    local horz = u.OnMotionHorzEventChange
    u.OnMotionHorzEventChange = function(self, new, old)
        LOG('PROBE ' .. tag .. ' ' .. (GetGameTick() - start) .. ' horz ' .. tostring(old) .. ' -> ' .. tostring(new))
        return horz(self, new, old)
    end
    table.insert(all, { tag = tag, u = u })
    return u
end
local function heading(u)
    local q = u:GetOrientation()
    return math.deg(math.atan2(2 * (q[4] * q[2] + q[1] * q[3]), 1 - 2 * (q[2] * q[2] + q[1] * q[1])))
end

local orders = {
    { spawn('line', 'url0107', 620, 80), 660, 80 },
    { spawn('behind', 'url0107', 650, 95), 630, 95 },
    { spawn('spin', 'url0107', 650, 110), 648, 110 },
    { spawn('side', 'url0107', 650, 125), 650, 119 },
    { spawn('marine', 'uel0106', 650, 140), 630, 140 },
}

ForkThread(function()
    WaitTicks(10)
    start = GetGameTick()
    for _, o in orders do IssueMove({ o[1] }, { o[2], 0, o[3] }) end
    for t = 0, TICKS do
        for _, e in all do
            local u = e.u
            local a = u.Animator
            local vx, vy, vz = u:GetVelocity()
            LOG('PROBE ' .. e.tag .. ' ' .. t .. ' anim ' .. (a and f3(a:GetAnimationTime()) or '-')
                .. ' speed ' .. f2(math.sqrt(vx * vx + vy * vy + vz * vz) * 10) .. ' heading ' .. f2(heading(u)))
        end
        WaitTicks(1)
    end
    LOG('PROBE done')
end)
