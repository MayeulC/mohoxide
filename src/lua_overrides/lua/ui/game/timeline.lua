-- The replay timeline (engine): a bar across a screen while a replay plays,
-- showing its length, the tick it's at and its major moments (an ACU
-- dying, an experimental finished, a nuke launched, or a mod's own; the
-- sim marks them in the recording with `Replay.AddEvent`).
--
-- Click the bar to go to that moment; click a marker to go to ten seconds
-- before it, with the main camera looking there. An earlier moment than now
-- restarts the replay from its latest keyframe before there (the whole game
-- saved every minute or so) and plays it on to there (`Replay.Seek`). The
-- lighter parts of the bar are where that's quick, like a video's buffered
-- part: they fill in as the replay is played ahead in the background
-- (`Replay.GetBuffered`, the Interface options "Replay: ...").
--
-- Built from the game's own pieces: the minimap's frame, the score panel's
-- replay-speed slider for the track, and the units' strategic icons in
-- their army's colour (as the world draws them zoomed out) for the events.
--
-- Which screen it shows on is the Interface option `replay_timeline`
-- ('main', 'second' or 'off'; without a second screen, 'second' shows on
-- the main one). Mods change the looks through `Kinds`, `Layout` and
-- `Textures`.

local UIUtil = import("/lua/ui/uiutil.lua")
local LayoutHelpers = import("/lua/maui/layouthelpers.lua")
local Group = import("/lua/maui/group.lua").Group
local Bitmap = import("/lua/maui/bitmap.lua").Bitmap
local Tooltip = import("/lua/ui/game/tooltip.lua")

--- Per event kind: its tooltip title, the colour of its tick on the track,
--- and an optional badge over the icon (a mod's kind without an entry uses
--- `Default`).
Kinds = {
    acu_death = { title = "<LOC timeline_0000>ACU destroyed", color = 'ffff4040', badge = '/game/unit-over/icon-skull_bmp.dds' },
    experimental_built = { title = "<LOC timeline_0001>Experimental built", color = 'ffffc040' },
    nuke_launch = { title = "<LOC timeline_0002>Nuke launched", color = 'ffff8020' },
}
Default = { color = 'ffc0c0c0' }

--- Sizes (the interface's pixels): the bar's height, its gap from the
--- screen's bottom and sides, the icons' scale; how far before an event a
--- marker's click lands (ticks).
Layout = { height = 74, bottom = 4, side = 8, iconScale = 1.25, lead = 100 }

--- The frame (the minimap's) and the track (the score panel's slider).
Textures = {
    tl = '/game/mini-map-brd/mini-map_brd_ul.dds',
    tr = '/game/mini-map-brd/mini-map_brd_ur.dds',
    tm = '/game/mini-map-brd/mini-map_brd_horz_um.dds',
    ml = '/game/mini-map-brd/mini-map_brd_vert_l.dds',
    m = '/game/mini-map-brd/mini-map_brd_m.dds',
    mr = '/game/mini-map-brd/mini-map_brd_vert_r.dds',
    bl = '/game/mini-map-brd/mini-map_brd_ll.dds',
    bm = '/game/mini-map-brd/mini-map_brd_lm.dds',
    br = '/game/mini-map-brd/mini-map_brd_lr.dds',
    track = '/dialogs/options/slider-back_bmp.dds',
    thumb = '/slider02/slider_btn_up.dds',
    seek = '/slider02/slider_btn_over.dds',
}

--- The score panel's game-time colour, and the rest.
local COLORS = { back = 'ff080c0e', time = 'ff00dbff', total = 'ffa0a0a0', played = '5000dbff', hover = '90ffffff', buffered = '38ffffff' }

--- The control showing (one at a time).
local timeline

--- `ticks` as the score panel shows game time: m:ss, or h:mm:ss.
local function clock(ticks)
    local s = math.floor(ticks / 10)
    local h, m = math.floor(s / 3600), math.floor(math.mod(s, 3600) / 60)
    s = math.mod(s, 60)
    if h > 0 then
        return string.format('%d:%02d:%02d', h, m, s)
    end
    return string.format('%d:%02d', m, s)
end

--- The option's screen: 0 the main one, 1 the second, nil off.
local function chosenScreen()
    local options = import("/lua/user/prefs.lua").GetFromCurrentProfile('options')
    local value = options and options.replay_timeline or 'main'
    if value == 'off' then return nil end
    if value == 'second' and GetNumRootFrames() > 1 then return 1 end
    return 0
end

local function army(index)
    local armies = GetArmiesTable().armiesTable
    return armies and armies[index] or {}
end

--- The unit's strategic icon (`StrategicIconName`), and its name.
local function unitInfo(blueprint)
    local bp = blueprint and __blueprints[blueprint]
    if not bp then return end
    local icon = bp.StrategicIconName and ('/textures/ui/common/game/strategicicons/' .. bp.StrategicIconName .. '_rest.dds')
    local name = bp.General and bp.General.UnitName and LOC(bp.General.UnitName) or (bp.Description and LOC(bp.Description))
    return icon, name
end

--- A bitmap of `texture` (an interface file) under `parent`, not taking
--- the mouse.
local function image(parent, texture)
    local b = Bitmap(parent, UIUtil.UIFile(texture))
    b:DisableHitTest()
    return b
end

--- Sets `c`'s edges and the size they make.
local function place(c, left, top, right, bottom)
    c.Left:Set(left)
    c.Top:Set(top)
    c.Right:Set(right)
    c.Bottom:Set(bottom)
    c.Width:Set(function() return c.Right() - c.Left() end)
    c.Height:Set(function() return c.Bottom() - c.Top() end)
end

--- The minimap's frame filling `panel`: corners at their size, edges and
--- middle stretched between.
local function frame(panel)
    local T = Textures
    local tl, tr, bl, br = image(panel, T.tl), image(panel, T.tr), image(panel, T.bl), image(panel, T.br)
    LayoutHelpers.AtLeftTopIn(tl, panel)
    LayoutHelpers.AtRightTopIn(tr, panel)
    LayoutHelpers.AtLeftBottomIn(bl, panel)
    LayoutHelpers.AtRightBottomIn(br, panel)
    local tm, bm, ml, mr, m = image(panel, T.tm), image(panel, T.bm), image(panel, T.ml), image(panel, T.mr), image(panel, T.m)
    place(tm, tl.Right, panel.Top, tr.Left, function() return panel.Top() + tm.BitmapHeight() end)
    place(bm, bl.Right, function() return panel.Bottom() - bm.BitmapHeight() end, br.Left, panel.Bottom)
    place(ml, panel.Left, tl.Bottom, function() return panel.Left() + ml.BitmapWidth() end, bl.Top)
    place(mr, function() return panel.Right() - mr.BitmapWidth() end, tr.Bottom, panel.Right, br.Top)
    place(m, ml.Right, tm.Bottom, mr.Left, bm.Top)
    for _, c in { tl, tr, bl, br, tm, bm, ml, mr, m } do
        c.Depth:Set(function() return panel.Depth() + 2 end)
    end
    -- The minimap's middle lets the world show through; here it's backed
    -- (as the score panel is).
    local back = Bitmap(panel)
    back:SetSolidColor(COLORS.back)
    back:DisableHitTest()
    place(back, ml.Left, tm.Top, mr.Right, bm.Bottom)
    back.Depth:Set(function() return panel.Depth() + 1 end)
    return m
end

--- Makes the timeline on `screen` (0 or 1).
local function Create(screen, events, length)
    local S = LayoutHelpers.ScaleNumber
    local parent = GetFrame(screen)
    local panel = Group(parent)
    panel.Depth:Set(function() return parent.Depth() + 100 end)
    place(panel,
        function() return parent.Left() + S(Layout.side) end,
        function() return parent.Bottom() - S(Layout.bottom + Layout.height) end,
        function() return parent.Right() - S(Layout.side) end,
        function() return parent.Bottom() - S(Layout.bottom) end)
    local client = frame(panel)
    local over = function() return panel.Depth() + 10 end

    local now = UIUtil.CreateText(panel, '0:00', 16, UIUtil.bodyFont)
    now:SetColor(COLORS.time)
    now:SetDropShadow(true)
    now:DisableHitTest()
    local total = UIUtil.CreateText(panel, clock(length), 14, UIUtil.bodyFont)
    total:SetColor(COLORS.total)
    total:SetDropShadow(true)
    total:DisableHitTest()
    LayoutHelpers.AtLeftIn(now, client, 6)
    LayoutHelpers.AtBottomIn(now, client, 4)
    LayoutHelpers.AtRightIn(total, client, 6)
    LayoutHelpers.AtBottomIn(total, client, 5)
    now.Depth:Set(over)
    total.Depth:Set(over)

    -- The track: the slider's groove, as long as the game; the played part
    -- tinted, the thumb where the replay is.
    local track = image(panel, Textures.track)
    place(track,
        function() return client.Left() + S(70) end,
        function() return now.Bottom() - S(4) - track.BitmapHeight() / 2 - S(4) end,
        function() return total.Left() - S(14) end,
        function() return now.Bottom() - S(4) + track.BitmapHeight() / 2 - S(4) end)
    track.Depth:Set(over)

    local played = Bitmap(panel)
    played:SetSolidColor(COLORS.played)
    played:DisableHitTest()
    played.Left:Set(track.Left)
    played.Top:Set(function() return track.Top() + S(2) end)
    played.Bottom:Set(function() return track.Bottom() - S(2) end)
    played.Height:Set(function() return played.Bottom() - played.Top() end)
    played.Width:Set(0)
    played.Depth:Set(function() return track.Depth() + 2 end)

    local function xOf(tick)
        local f = length > 0 and math.min(1, math.max(0, tick / length)) or 0
        return track.Left() + f * track.Width()
    end
    local function tickAt(x)
        local f = (x - track.Left()) / math.max(1, track.Width())
        return math.floor(math.min(1, math.max(0, f)) * length + 0.5)
    end

    -- Where a jump is quick (`Replay.GetBuffered`): lighter bars along the
    -- groove, under the played part.
    local buffered = {}
    local function showBuffered()
        local ranges = Replay.GetBuffered()
        for i, r in ranges do
            local bar = buffered[i]
            if not bar then
                bar = Bitmap(panel)
                bar:SetSolidColor(COLORS.buffered)
                bar:DisableHitTest()
                bar.Top:Set(function() return track.Top() + S(2) end)
                bar.Bottom:Set(function() return track.Bottom() - S(2) end)
                bar.Height:Set(function() return bar.Bottom() - bar.Top() end)
                bar.Depth:Set(function() return track.Depth() + 1 end)
                buffered[i] = bar
            end
            local from, to = r.from, r.to
            bar.Left:Set(function() return xOf(from) end)
            bar.Width:Set(function() return math.max(S(1), xOf(to) - xOf(from)) end)
            bar:Show()
        end
        for i = table.getn(ranges) + 1, table.getn(buffered) do
            buffered[i]:Hide()
        end
    end
    showBuffered()

    local thumb = image(panel, Textures.thumb)
    thumb.Depth:Set(function() return track.Depth() + 6 end)
    LayoutHelpers.AtVerticalCenterIn(thumb, track)
    local seekThumb = image(panel, Textures.seek)
    seekThumb.Depth:Set(function() return track.Depth() + 5 end)
    LayoutHelpers.AtVerticalCenterIn(seekThumb, track)
    seekThumb:Hide()

    -- Where the mouse is on the track, with its time above it.
    local hoverLine = Bitmap(panel)
    hoverLine:SetSolidColor(COLORS.hover)
    hoverLine:DisableHitTest()
    hoverLine.Width:Set(S(1))
    hoverLine.Top:Set(track.Top)
    hoverLine.Bottom:Set(track.Bottom)
    hoverLine.Height:Set(track.Height)
    hoverLine.Depth:Set(function() return track.Depth() + 4 end)
    hoverLine:Hide()
    local label = UIUtil.CreateText(panel, '', 12, UIUtil.bodyFont)
    label:SetColor(COLORS.time)
    label:SetDropShadow(true)
    label:DisableHitTest()
    label.Bottom:Set(function() return track.Top() - S(1) end)
    label.Depth:Set(function() return track.Depth() + 8 end)
    label:Hide()

    -- Clicks and hover on the track (a taller strip than the groove, so
    -- it's easy to hit).
    local strip = Bitmap(panel)
    strip:SetSolidColor('00000000')
    place(strip, track.Left, function() return track.Top() - S(6) end, track.Right, function() return track.Bottom() + S(6) end)
    strip.Depth:Set(function() return track.Depth() + 3 end)
    strip.HandleEvent = function(self, event)
        if event.Type == 'MouseMotion' or event.Type == 'MouseEnter' then
            local x = math.max(track.Left(), math.min(event.MouseX, track.Right()))
            hoverLine.Left:Set(x)
            hoverLine:Show()
            label:SetText(clock(tickAt(x)))
            label.Left:Set(x - label.Width() / 2)
            label:Show()
            return true
        elseif event.Type == 'MouseExit' then
            hoverLine:Hide()
            label:Hide()
            return true
        elseif event.Type == 'ButtonPress' then
            Replay.Seek(tickAt(event.MouseX))
            return true
        end
    end

    -- The events: the unit's strategic icon in its army's colour above the
    -- track, a tick down to the groove in the kind's colour.
    for _, e in events do
        local event = e
        local kind = Kinds[event.kind] or Default
        local x = function() return xOf(event.tick) end
        local tick = Bitmap(panel)
        tick:SetSolidColor(kind.color)
        tick:DisableHitTest()
        tick.Width:Set(S(2))
        tick.Left:Set(function() return x() - S(1) end)
        tick.Top:Set(function() return track.Top() - S(6) end)
        tick.Bottom:Set(track.Bottom)
        tick.Height:Set(function() return tick.Bottom() - tick.Top() end)
        tick.Depth:Set(function() return track.Depth() + 2 end)

        local iconPath, unitName = unitInfo(event.blueprint)
        local icon = Bitmap(panel, iconPath)
        if iconPath then
            icon:SetColorMask(army(event.army).color or 'ffffffff')
        else
            icon:SetSolidColor(kind.color)
        end
        icon.Width:Set(function() return S(Layout.iconScale * math.max(icon.BitmapWidth(), 16)) end)
        icon.Height:Set(function() return S(Layout.iconScale * math.max(icon.BitmapHeight(), 16)) end)
        icon.Left:Set(function() return x() - icon.Width() / 2 end)
        icon.Bottom:Set(function() return track.Top() - S(7) end)
        icon.Depth:Set(function() return track.Depth() + 7 end)
        if kind.badge then
            local badge = image(panel, kind.badge)
            badge.Width:Set(S(14))
            badge.Height:Set(S(14))
            badge.Left:Set(function() return icon.Right() - S(8) end)
            badge.Top:Set(function() return icon.Top() - S(6) end)
            badge.Depth:Set(function() return icon.Depth() + 1 end)
        end
        icon.HandleEvent = function(self, ev)
            if ev.Type == 'ButtonPress' then
                Replay.Seek(math.max(0, event.tick - Layout.lead), event.position)
                return true
            end
        end
        local who = army(event.army).nickname or ('Army ' .. tostring(event.army))
        local body = clock(event.tick) .. '  ' .. who .. (unitName and ('\n' .. unitName) or '')
        Tooltip.AddControlTooltipManual(icon, LOC(kind.title or event.kind), body)
    end

    -- Each frame: the played part, the clock, and where a seek goes (the
    -- engine knows: a seek back restarts the replay and its interface).
    panel:SetNeedsFrameUpdate(true)
    local sinceBuffered = 0
    panel.OnFrame = function(self, delta)
        -- The keyframes on disk change as the replay is played ahead.
        sinceBuffered = sinceBuffered + delta
        if sinceBuffered >= 1 then
            sinceBuffered = 0
            showBuffered()
        end
        local t = GameTick()
        local x = xOf(t)
        played.Width:Set(math.max(0, x - track.Left()))
        thumb.Left:Set(x - thumb.Width() / 2)
        local target = Replay.GetSeekTarget()
        if target then
            seekThumb.Left:Set(xOf(target) - seekThumb.Width() / 2)
            seekThumb:Show()
            now:SetText(clock(t) .. ' > ' .. clock(target))
        else
            seekThumb:Hide()
            now:SetText(clock(t))
        end
    end
    return panel
end

--- Removes the timeline, if one shows.
function Destroy()
    if timeline then
        timeline:Destroy()
        timeline = nil
    end
end

--- (Re)makes the timeline per the option: in a replay with a recorded
--- length, on the chosen screen.
function Refresh()
    Destroy()
    if not SessionIsReplay() then return end
    local screen = chosenScreen()
    local length = Replay.GetLength()
    if not screen or length <= 0 then return end
    timeline = Create(screen, Replay.GetEvents(), length)
end

--- The game interface calls this once it's made.
function Init()
    Refresh()
end
