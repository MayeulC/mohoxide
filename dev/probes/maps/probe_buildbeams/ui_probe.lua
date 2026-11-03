-- Interface half of the build beams probe: logs the graphics options,
-- points the world camera at the engineers and, at tick 300, pauses and logs
-- `PROBE shot <view>` for each view in turn (run_faf.ps1 captures the screen
-- for each), then `PROBE done`. Ours takes the views with --camera /
-- --screenshot instead: with `PROBE_NO_PAUSE = true` this only sets the
-- camera to the view named by PROBE_VIEW (default all).
local options = import('/lua/user/prefs.lua').GetFromCurrentProfile('options') or {}
LOG('PROBE options fidelity ' .. tostring(options.fidelity) .. ' bloom ' .. tostring(options.bloom_render)
    .. ' shadows ' .. tostring(options.shadow_quality) .. ' aa ' .. tostring(options.antialiasing))

-- Ground height there 18.5; heading pi faces north, pitch down from level.
local views = {
    { 'all', 394, 610, 40 },
    { 'uef', 379, 610, 14 },
    { 'cybran', 389, 610, 14 },
    { 'aeon', 399, 610, 14 },
    { 'seraphim', 409, 610, 14 },
    { 'cybran-overlay', 389.5, 606.5, 10 },
    { 'cybran-factory', 424.5, 607, 14 },
}
local function look(v)
    GetCamera('WorldCamera'):SnapTo(Vector(v[2], 18.5, v[3]), { math.pi, 1.0, 0 }, v[4] * (rawget(_G, 'PROBE_ZOOM_SCALE') or 1))
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
