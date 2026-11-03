-- Interface half of the Cybran build probe: logs the graphics options,
-- points the world camera at the four power generators going up and, at tick
-- 300, pauses and logs `PROBE shot <view>` for each view (run_faf.ps1
-- captures the screen for each), then `PROBE done`. Ours takes the views with
-- --screenshot instead: with `PROBE_NO_PAUSE = true` this only sets the
-- camera to the view named by PROBE_VIEW (default all).
local options = import('/lua/user/prefs.lua').GetFromCurrentProfile('options') or {}
LOG('PROBE options fidelity ' .. tostring(options.fidelity) .. ' bloom ' .. tostring(options.bloom_render)
    .. ' shadows ' .. tostring(options.shadow_quality) .. ' aa ' .. tostring(options.antialiasing))

-- Ground height there about 18.5; heading pi faces north, pitch down from
-- level (1.2: nearly overhead, so the ground under each one shows).
local views = {
    { 'all', 386.5, 609.5, 22 },
    { 'site1', 383, 606, 9 },
    { 'site2', 390, 606, 9 },
    { 'site3', 383, 613, 9 },
    { 'site4', 390, 613, 9 },
}
local function look(v)
    GetCamera('WorldCamera'):SnapTo(Vector(v[2], 18.5, v[3]), { math.pi, 1.2, 0 }, v[4] * (rawget(_G, 'PROBE_ZOOM_SCALE') or 1))
end

if rawget(_G, 'PROBE_NO_PAUSE') then
    for _, v in views do
        if v[1] == (rawget(_G, 'PROBE_VIEW') or 'all') then look(v) end
    end
else
    look(views[1])
    ForkThread(function()
        while GameTick() < 300 do WaitSeconds(0.1) end
        SessionRequestPause()
        LOG('PROBE paused ' .. GameTick())
        WaitSeconds(1)
        for _, v in views do
            look(v)
            WaitSeconds(1.5)
            LOG('PROBE shot ' .. v[1])
            WaitSeconds(4)
        end
        LOG('PROBE done')
    end)
end
