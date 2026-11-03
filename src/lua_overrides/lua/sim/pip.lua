-- Picture-in-picture popups, the sim's side (engine): `Request` asks the
-- interfaces that may know of an event to show a unit in a popup
-- (`/lua/ui/game/pip.lua`). The unit's own side and its allies hear of it,
-- and observers; other players' interfaces never do, so a popup can't scout.
--
-- The request travels in `Sync.PiPRequests`, which isn't sim state: what's
-- written there may differ between clients (as FAF's notify messages do),
-- nothing in the sim reads it back.
--
-- Mods call it for their own events:
--     import('/lua/sim/pip.lua').Request(unit, 'my_event', { zoom = 30, duration = 6 })

--- Asks for a popup on `unit`.
---@param unit Unit
---@param reason string      # 'acu_death', 'experimental_started', or a mod's own
---@param opts? table        # passed on to `PiP.Show`: zoom, duration, title, priority, mode
function Request(unit, reason, opts)
    local army = unit.Army
    local focus = GetFocusArmy()
    if focus ~= -1 and focus ~= army and not IsAlly(focus, army) then
        return
    end
    local pos = unit:GetPosition()
    local request = {
        reason = reason,
        army = army,
        blueprint = unit.UnitId,
        entities = { unit:GetEntityId() },
        position = { pos[1], pos[2], pos[3] },
    }
    for k, v in opts or {} do
        request[k] = v
    end
    local requests = Sync.PiPRequests or {}
    Sync.PiPRequests = requests
    table.insert(requests, request)
end
