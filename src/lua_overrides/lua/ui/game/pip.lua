-- Picture-in-picture popups (engine): a small world view on its own camera
-- that follows a unit for a few seconds, e.g. an ACU dying or an
-- experimental being started. Built only from moho's own pieces (a
-- `UIWorldView`, `GetCamera(name):TrackEntities`), so it runs in FAF too.
--
-- The sim asks for them through `/lua/sim/pip.lua` (`Sync.PiPRequests`);
-- interface code and mods can open one directly:
--
--     local PiP = import('/lua/ui/game/pip.lua')
--     local popup = PiP.Show { entities = { id }, title = 'Look!', duration = 6 }
--     PiP.Close(popup)
--
-- Mods change the defaults through `Reasons` (per-reason settings; `false`
-- turns one off), `SetSlots`, `Anchor`, or by replacing `OnClick`.

local UIUtil = import("/lua/ui/uiutil.lua")
local LayoutHelpers = import("/lua/maui/layouthelpers.lua")
local Control = import("/lua/maui/control.lua").Control
local Bitmap = import("/lua/maui/bitmap.lua").Bitmap
local Window = import("/lua/maui/window.lua").Window

--- A world view showing camera `cameraName`, nothing else: the popup's
--- picture (FAF's `WorldView` brings cursors and orders, which a popup
--- doesn't want).
local PiPView = ClassUI(moho.UIWorldView, Control) {}

--- Settings per reason the sim gives (`Request(unit, reason)`): what a
--- popup for it looks like. `false` turns the reason off; a reason with no
--- entry uses `Default`.
Reasons = {
    acu_death = { title = "<LOC pip_0000>ACU destroyed", zoom = 15, duration = 8, priority = 20 },
    experimental_started = { title = "<LOC pip_0001>Experimental started", zoom = 70, duration = 10, priority = 10 },
}
Default = { zoom = 50, duration = 8, priority = 0 }

--- Where the popups open until a player moves them (each slot then keeps
--- its own place, `pip_window_<slot>` in the prefs): from the screen's
--- right edge and bottom, stacking upwards; the picture's size; in the
--- interface's pixels. `scoreboard`: on the main screen, as wide as the
--- score panel and under its right edge (the picture keeps `width` x
--- `height`'s shape); without one, `right` and `width`.
Anchor = { right = 12, bottom = 170, width = 240, height = 150, gap = 6, scoreboard = true }

--- The minimap's frame (FAF's `minimap.lua`).
Textures = {
    tl = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_ul.dds'),
    tr = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_ur.dds'),
    tm = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_horz_um.dds'),
    ml = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_vert_l.dds'),
    m = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_m.dds'),
    mr = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_vert_r.dds'),
    bl = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_ll.dds'),
    bm = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_lm.dds'),
    br = UIUtil.UIFile('/game/mini-map-brd/mini-map_brd_lr.dds'),
    borderColor = 'ff415055',
}

--- The window's frame around the picture: the title bar (`Window`'s) and
--- the sides.
local TITLE, SIDE = 30, 6

local slots = 2
--- The popups showing, oldest first.
local open = {}

--- How many popups can show at once (a new one replaces the lowest
--- priority, then the oldest, when they're full).
function SetSlots(n)
    slots = math.max(0, n)
    while table.getn(open) > slots do
        Close(open[1])
    end
end

--- The popups showing, oldest first (a copy).
function Popups()
    local list = {}
    for i, p in open do list[i] = p end
    return list
end

--- What clicking a popup's picture does: the main view goes there.
function OnClick(popup)
    local at = popup.Position
    if popup.Entities and popup.Entities[1] then
        local unit = GetUnitById(tostring(popup.Entities[1]))
        if unit then
            at = unit:GetPosition()
        end
    end
    if at then
        local cam = GetCamera('WorldCamera')
        cam:MoveTo(Vector(at[1], at[2], at[3]), nil, popup.Zoom or cam:GetZoom(), 0.5)
    end
end

--- The score panel on `screen` (FAF's `score.lua`; the main screen only),
--- if it shows.
local function scoreboard(screen)
    if screen ~= 0 or not Anchor.scoreboard then return end
    local ok, score = pcall(import, '/lua/ui/game/score.lua')
    local bg = ok and score.controls and score.controls.bg
    if bg and not IsDestroyed(bg) and bg.Width() > 0 then return bg end
end

--- The lowest slot no popup has: its camera (`PiP<slot>`, reused: moho
--- keeps a camera once a view names it) and its place.
local function freeSlot()
    local slot = 1
    while true do
        local used = false
        for _, p in open do
            if p.Slot == slot then used = true end
        end
        if not used then return slot end
        slot = slot + 1
    end
end

--- Closes `popup` (a popup `Show` returned, or its `id`).
function Close(popup)
    for i, p in open do
        if p == popup or (type(popup) == 'string' and p.Id == popup) then
            table.remove(open, i)
            p:Destroy()
            return
        end
    end
end

--- Opens a popup.
---@param params table
---  entities?  entity ids (or user units) to follow
---  position?  {x, y, z} to look at (without entities, or while they load)
---  zoom?      camera distance
---  duration?  seconds before it closes itself (nil: until closed)
---  title?     text across its top
---  priority?  a full set of popups gives way to a higher one
---  id?        a popup with the same id is replaced instead of stacking
---  screen?    the monitor it opens on: 0 the main window, 1 the second screen (FAF's
---             second head, `GetFrame(1)`); without that screen, the main window
---@return Window? popup
function Show(params)
    if slots == 0 then return end
    if params.id then Close(params.id) end
    local priority = params.priority or 0
    if table.getn(open) >= slots then
        local lowest
        for _, p in open do
            if not lowest or p.Priority < lowest.Priority then lowest = p end
        end
        if lowest.Priority > priority then return end
        Close(lowest)
    end
    local screen = params.screen or 0
    if screen >= GetNumRootFrames() then screen = 0 end
    local parent = GetFrame(screen)
    local slot = freeSlot()

    -- Where this slot opens by default: stacked up from the anchor.
    local S = LayoutHelpers.ScaleNumber
    local board = scoreboard(screen)
    local w = board and LayoutHelpers.InvScaleNumber(board.Width()) or Anchor.width + 2 * SIDE
    local h = math.floor((w - 2 * SIDE) * Anchor.height / Anchor.width + 0.5) + TITLE + SIDE
    local bottom = function() return parent.Bottom() - S(Anchor.bottom + (slot - 1) * (h + Anchor.gap)) end
    local right = board and function() return board.Right() end or function() return parent.Right() - S(Anchor.right) end
    local defaultPosition = {
        Left = function() return right() - S(w) end,
        Top = function() return bottom() - S(h) end,
        Right = right,
        Bottom = bottom,
    }
    local popup = Window(parent, params.title or '', nil, false, false, true, false, 'pip_window_' .. slot, defaultPosition, Textures)
    popup.Depth:Set(function() return parent.Depth() + 50 end)
    -- A moved popup's saved place keeps its corner, not its size (one saved
    -- before `Anchor` changed would open at the old size).
    if math.abs(popup.Width() - S(w)) > 1 or math.abs(popup.Height() - S(h)) > 1 then
        local left = math.max(parent.Left(), math.min(popup.Left(), parent.Right() - S(w)))
        local top = math.max(parent.Top(), math.min(popup.Top(), parent.Bottom() - S(h)))
        popup.Left:Set(left)
        popup.Top:Set(top)
        popup.Right:Set(left + S(w))
        popup.Bottom:Set(top + S(h))
    end
    popup.Slot = slot
    popup.Id = params.id
    popup.Priority = priority
    popup.Entities = params.entities
    popup.Position = params.position
    popup.Zoom = params.zoom
    popup.CameraName = 'PiP' .. slot
    -- The window's middle draws in the world views' pass, under the
    -- picture (as the minimap's).
    popup.window_m:SetRenderPass(UIUtil.UIRP_UnderWorld)
    popup.OnClose = function(self) Close(self) end
    -- (`Window`'s title font is sized for wide windows.)
    popup._title:SetFont(UIUtil.bodyFont, 11)
    LayoutHelpers.AtLeftTopIn(popup._title, popup.TitleGroup, 10, 9)

    local client = popup:GetClientGroup()
    local view = PiPView(client, popup.CameraName, 1, false)
    LayoutHelpers.FillParent(view, client)
    view:LockInput()

    -- Clicks on the picture (the view's input is locked: it gives no orders).
    local catcher = Bitmap(client)
    catcher:SetSolidColor('00000000')
    LayoutHelpers.FillParent(catcher, client)
    catcher.Depth:Set(function() return view.Depth() + 1 end)
    catcher.HandleEvent = function(self, event)
        if event.Type == 'ButtonPress' then
            OnClick(popup)
            return true
        end
    end

    local cam = GetCamera(popup.CameraName)
    if params.position then
        local p = params.position
        cam:SnapTo(Vector(p[1], p[2], p[3]), nil, params.zoom)
    end
    if params.entities and params.entities[1] then
        cam:TrackEntities(params.entities, params.zoom, 0)
    end

    if params.duration then
        popup:SetNeedsFrameUpdate(true)
        local left = params.duration
        popup.OnFrame = function(self, delta)
            left = left - delta
            if left <= 0 then Close(self) end
        end
    end

    table.insert(open, popup)
    return popup
end

--- The player's option for a reason (Interface tab, `pip_<reason>`): on
--- unless set to 0 (a mod's reason has none).
local function optionOn(reason)
    local options = import("/lua/user/prefs.lua").GetFromCurrentProfile('options')
    return not options or options['pip_' .. reason] ~= 0
end

--- The sim's requests (`/lua/sim/pip.lua`): shown if this interface may
--- know of them (the focus army's side, or observing; the focus army can
--- change after the sim wrote them) and their reason isn't turned off.
function OnRequests(requests)
    local focus = GetFocusArmy()
    for _, r in requests do
        local settings = Reasons[r.reason]
        if settings == nil then settings = Default end
        if settings and optionOn(r.reason) and (focus == -1 or focus == r.army or IsAlly(focus, r.army)) then
            local params = { id = r.reason .. ':' .. tostring(r.entities and r.entities[1]) }
            for k, v in settings do params[k] = v end
            for k, v in r do
                if k ~= 'reason' and k ~= 'army' and k ~= 'blueprint' then params[k] = v end
            end
            if params.title then params.title = LOC(params.title) end
            Show(params)
        end
    end
end

--- Starts listening for the sim's requests (the game interface does this
--- once it's made).
function Init()
    AddOnSyncHashedCallback(OnRequests, 'PiPRequests', 'pip')
end
