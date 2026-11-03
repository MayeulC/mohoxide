-- A UEF land factory with a build queue of three entries (tanks x2,
-- engineer, scout), for FAF's draggable build queue (option
-- gui_draggable_queue; dev/scenes/ui/queue_drag.prefs turns it on). With
-- dev/scenes/ui/queue_drag_ui.lua as --ui-lua the interface drags the first entry
-- onto the last and logs the queue before and after (QUEUEDRAG lines):
-- cargo run -- --no-ai --player 1 --script dev/scenes/ui/queue_drag.lua --prefs dev/scenes/ui/queue_drag.prefs \
--   --camera 400,600,60 --ui-lua "$(cat dev/scenes/ui/queue_drag_ui.lua)"
ForkThread(function()
    WaitTicks(2)
    local f = CreateUnitHPR('ueb0101', 'ARMY_1', 400, GetTerrainHeight(400, 600), 600, 0, 0, 0)
    LOG('QUEUEDRAG factory ', f:GetEntityId())
    IssueBuildFactory({ f }, 'uel0201', 2)
    IssueBuildFactory({ f }, 'uel0105', 1)
    IssueBuildFactory({ f }, 'uel0101', 1)
end)
