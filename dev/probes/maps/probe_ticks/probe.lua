-- Differential probe: how many game ticks each kind of wait takes.
--   real game (Windows): dev/probes/run_faf.ps1 probe_ticks 60
--   ours: cargo run -- --headless --no-ai --ticks 200 --script dev/probes/maps/probe_ticks/probe.lua
local function try(name, f)
    local t = GetGameTick()
    f()
    LOG('PROBE ' .. name .. ' ' .. (GetGameTick() - t))
end
ForkThread(function()
    WaitTicks(1)
    for _, n in { 0, 1, 2, 3, 5, 10 } do try('WaitTicks(' .. n .. ')', function() WaitTicks(n) end) end
    for _, n in { 0, 0.05, 0.1, 0.15, 0.2, 0.5, 1 } do try('WaitSeconds(' .. n .. ')', function() WaitSeconds(n) end) end
    for _, n in { 0, 1, 2, 3, 2.5, 3.9 } do try('yield(' .. n .. ')', function() coroutine.yield(n) end) end
    -- Last: in the real game a bare yield doesn't come back (seen once).
    ForkThread(function() WaitTicks(51); LOG('PROBE done') if EndGame then EndGame() end end)
    try('yield()', function() coroutine.yield() end)
end)
