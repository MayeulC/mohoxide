-- Scenario test (tests/scenarios.rs): a submarine dives and surfaces bow
-- first. Moho's `CUnitMotion::CalcRollHack` leans the up axis
-- along the nose by four times the tick's depth step, smoothed
-- (0.25 target + 0.75 last), and `SnapToWater` tilts the transform by it:
-- nose down diving, nose up surfacing, easing level after. The numbers are
-- the real game's (dev/probes/maps/probe_dive): the nose's height peaks at
-- -0.3425 diving and +0.3425 surfacing, the transition takes 93 ticks, it
-- eases back from the arrival tick (-0.0306 then) and stops easing at
-- -0.00097, which it keeps.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do
            for x = 100, w - 100, 8 do
                if not site and depth(x, z) > 10 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        local sub = CreateUnitHPR('ues0401', 'ARMY_1', sx, GetSurfaceHeight(sx, sz), sz, 0, 0.7, 0)
        local heading = sub:GetHeading()
        WaitTicks(2)
        -- The nose's height: forward (0, 0, 1) turned by the orientation.
        local function pitch()
            local q = sub:GetOrientation()
            return 2 * (q[2] * q[3] - q[4] * q[1])
        end
        -- Its heading, from the nose's level part.
        local function nose_heading()
            local q = sub:GetOrientation()
            local fx, fz = 2 * (q[1] * q[3] + q[4] * q[2]), 1 - 2 * (q[1] * q[1] + q[2] * q[2])
            return math.atan2(fx, fz)
        end
        local function level() return math.abs(pitch()) < 2e-3 end
        local nose0 = nose_heading()
        check(level(), 'level before the dive: ' .. pitch())

        local function transition(state, layer)
            local lowest, highest, ticks = 0, 0, 0
            for _ = 1, 300 do
                WaitTicks(1)
                ticks = ticks + 1
                lowest, highest = math.min(lowest, pitch()), math.max(highest, pitch())
                if not sub:IsUnitState(state) and sub:GetCurrentLayer() == layer then break end
            end
            return lowest, highest, ticks
        end
        local function settle()
            for i = 1, 60 do
                if level() then return i end
                WaitTicks(1)
            end
        end
        local function near(v, want, tol) return math.abs(v - want) <= tol end
        -- The lean it keeps once it stops easing back (moho's 1e-6 cutoff).
        local function residual(sign)
            WaitTicks(20)
            local a = pitch()
            WaitTicks(10)
            check(near(a, sign * 0.00097, 0.0002) and a == pitch(), 'keeps the last lean, ' .. sign * 0.00097 .. ': ' .. a .. ' then ' .. pitch())
        end

        IssueDive({ sub })
        local lowest, highest, ticks = transition('MovingDown', 'Sub')
        note(string.format('dive: %d ticks, nose %.3f..%.3f', ticks, lowest, highest))
        check(sub:GetCurrentLayer() == 'Sub', 'it dives')
        check(ticks == 93, 'dives in 93 ticks: ' .. ticks)
        check(near(lowest, -0.3425, 0.001), 'nose down diving, peak -0.3425: ' .. lowest)
        check(highest <= 1e-4, 'never nose up diving: ' .. highest)
        check(math.abs(nose_heading() - nose0) < 1e-3, 'the heading holds through the dive: ' .. nose_heading())
        check(near(pitch(), -0.0306, 0.0005), 'easing back from the arrival tick, -0.0306: ' .. pitch())
        local settled = settle()
        note('level again after ' .. tostring(settled) .. ' ticks')
        check(settled ~= nil and settled > 5, 'eases level after the dive, not at once: ' .. tostring(settled))
        residual(-1)
        check(math.abs(sub:GetHeading() - heading) < 1e-3, 'facing unchanged: ' .. sub:GetHeading())

        IssueDive({ sub })
        lowest, highest, ticks = transition('MovingUp', 'Water')
        note(string.format('surface: %d ticks, nose %.3f..%.3f', ticks, lowest, highest))
        check(sub:GetCurrentLayer() == 'Water', 'it surfaces')
        check(ticks == 93, 'surfaces in 93 ticks: ' .. ticks)
        check(near(highest, 0.3425, 0.001), 'nose up surfacing, peak 0.3425: ' .. highest)
        -- (Its first tick still carries the dive's last lean, -0.00097.)
        check(lowest >= -0.0011, 'never nose down surfacing: ' .. lowest)
        check(near(pitch(), 0.0306, 0.0005), 'easing back from the arrival tick, 0.0306: ' .. pitch())
        settled = settle()
        check(settled ~= nil, 'level again after surfacing')
        residual(1)

        -- It still turns afterwards (its facing is kept apart from the
        -- tilted orientation from the first dive on).
        local p = sub:GetPosition()
        IssueMove({ sub }, { p[1] - 30, p[2], p[3] - 30 })
        WaitTicks(60)
        check(math.abs(nose_heading() - nose0) > 0.5, 'turns to a move order after a dive: ' .. nose_heading() .. ' from ' .. nose0)
        check(level(), 'level while turning: ' .. pitch())
    end)
    if not ok then check(false, 'error: ' .. tostring(err)) end
    ScenarioResult.done = true
end)
