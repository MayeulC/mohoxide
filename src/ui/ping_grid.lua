-- The ping grid (Ctrl+Shift+P, in the lobby and in game): every player's
-- round trip to every other, row = the player measuring, column = the one
-- measured, each cell coloured by its smoothed round trip (hover one for its
-- 99th percentile and route). Each client measures its own
-- row and the others send theirs once a second (`net::core` `Msg::Pings`),
-- so A->B and B->A both show and a one-sided link stands out. Built from
-- FAF's maui controls (plus the engine's `Text:SetVertical` for the column
-- names); none of FAF's files change.
--
-- In a game each player's own cell (the diagonal) shows their machine's
-- top speed: the fastest game speed it keeps up with (`net::lockstep`
-- capacity), purple for the one limiting the game below the speed asked
-- for; the bottom lines say the speed asked for, the speed now, and the
-- fastest possible (the slowest machine's top speed) and whose it is. The column on the right is each player's input delay: how far
-- ahead their orders are scheduled (`net::lockstep`), in ms at this speed.
--
-- It sizes itself to the screen: the largest names and cells (up to
-- CELL_MAX) that fit, so the same window serves 2 players and 63.
-- `RTS_PING_GRID_FAKE=<players>` fills it with made up players to see it.

local grid = false

local PAD, TITLE_H, FOOTER_H, MIN_W, TOP = 12, 24, 94, 480, 84
local NAME_CHARS, CELL_MAX = 16, 24

local function colorFor(ms)
    if ms < 80 then return 'ff64d264' end
    if ms < 150 then return 'ffffc726' end
    return 'ffff5050'
end

local function short(name, max)
    if string.len(name) > max then return string.sub(name, 1, max - 1) .. '.' end
    return name
end

local function ms(v)
    if v < 1 then return '<1' end
    return string.format('%d', math.floor(v + 0.5))
end

-- Moho's speed steps: rate 10^(step/10).
local function steps(step)
    local x = (string.gsub(string.format('%.2f', 10 ^ (step / 10)), '%.?0+$', ''))
    return string.format('%sx (%+d)', x, step)
end

-- The speed the game runs at now, as a multiplier (`GetSimRate` is a step).
local function rateNow()
    return 10 ^ ((GetSimRate and GetSimRate() or 0) / 10)
end

-- A player's input delay as time at the speed the game runs at now.
local function delayText(ticks)
    if not ticks then return '-' end
    return string.format('%d ms', math.floor(ticks * 100 / rateNow() + 0.5))
end

-- The speed asked for (step), the speed now (step) and the lowest top speed
-- (step and who), in a game; nil in the lobby (no top speeds yet).
local function speeds(data)
    local low, who
    for _, p in data.players do
        if p.max and (not low or p.max < low) then low, who = p.max, p.name end
    end
    if not low then return nil end
    local asked = GetGameSpeed and GetGameSpeed() or 0
    local now = GetSimRate and GetSimRate() or 0
    return asked, now, low, who
end

local function signature(data)
    local s = ''
    for _, p in data.players do s = s .. p.id .. '=' .. p.name .. ';' end
    return s
end

-- A hidden text to measure strings with: (advance, line height) at `size`.
local function measurer(parent)
    local UIUtil = import('/lua/ui/uiutil.lua')
    local probe = UIUtil.CreateText(parent, '', 12, UIUtil.bodyFont)
    probe:Hide()
    local size = 12
    return function(s, at)
        if at ~= size then
            probe:SetFont(UIUtil.bodyFont, at)
            size = at
        end
        probe:SetText(s)
        return probe.TextAdvance(), probe.FontAscent() + probe.FontDescent()
    end, probe
end

-- How big everything is for `n` players with these names in `availW` x
-- `availH`: {font, cell, nameW, headerH}, the largest font whose names leave
-- room for cells at least a line tall.
local function plan(names, n, availW, availH, measure)
    local function longest(f)
        local w = 0
        for _, name in names do w = math.max(w, (measure(name, f))) end
        return w
    end
    local function fitting(nameW, headerH, delayW, speedW)
        return math.floor(math.min((availW - PAD * 2 - nameW - delayW - speedW) / n, (availH - PAD * 2 - TITLE_H - headerH - FOOTER_H) / n))
    end
    for f = 12, 6, -1 do
        local _, lineH = measure('Ag', f)
        local nameW = math.ceil(longest(f) + 8)
        local headerH = math.ceil(longest(f) + 6)
        local delayW = math.ceil(math.max(measure('9999 ms', f), measure('input delay', f)) + 10)
        local speedW = math.ceil(math.max(measure('99.99x', f), measure('top speed', f)) + 10)
        local cell = fitting(nameW, headerH, delayW, speedW)
        if cell >= math.ceil(lineH + 2) or f == 6 then
            return {font = f, cell = math.max(math.min(cell, CELL_MAX), 4), nameW = nameW, headerH = headerH, delayW = delayW, speedW = speedW}
        end
    end
end

-- (Re)builds the window's controls for this set of players.
local function build(data)
    local UIUtil = import('/lua/ui/uiutil.lua')
    local LayoutHelpers = import('/lua/maui/layouthelpers.lua')
    local Bitmap = import('/lua/maui/bitmap.lua').Bitmap
    if grid.body then grid.body:Destroy() end
    grid.cells, grid.hover = {}, false
    local n = data and table.getn(data.players) or 0
    local scale = LayoutHelpers.GetPixelScaleFactor()
    local availW = grid.root.Width() / scale - 2 * PAD
    local measure, probe = measurer(grid.root)
    local names = {}
    if data then
        for i, p in data.players do names[i] = short(p.name, NAME_CHARS) end
    end
    -- Below the game's speed / pause buttons when it fits there, else from
    -- the top of the screen.
    local top = TOP
    local p = {font = 12, cell = 0, nameW = 0, headerH = 0, delayW = 0, speedW = 0}
    if n > 0 then
        p = plan(names, n, availW, grid.root.Height() / scale - TOP - PAD, measure)
        if p.cell < CELL_MAX then
            top = PAD
            p = plan(names, n, availW, grid.root.Height() / scale - top - PAD, measure)
        end
    end
    local cell = p.cell
    probe:Destroy()
    grid.plan = p
    local w = math.max(MIN_W, PAD * 2 + p.nameW + n * cell + p.delayW + p.speedW)
    local h = PAD * 2 + TITLE_H + p.headerH + n * cell + FOOTER_H
    if n == 0 then h = PAD * 2 + TITLE_H + 20 + FOOTER_H end
    local body = Bitmap(grid.root)
    body:SetSolidColor('ff0b0f14')
    LayoutHelpers.SetDimensions(body, w, h)
    -- Top centre (clear of the score panel and the lobby's slots).
    LayoutHelpers.AtHorizontalCenterIn(body, grid.root)
    LayoutHelpers.AtTopIn(body, grid.root, top)
    body.Depth:Set(10000)
    grid.body = body
    local function text(s, size, color, x, y)
        local t = UIUtil.CreateText(body, s, size, UIUtil.bodyFont)
        t:SetColor(color)
        t:SetDropShadow(true)
        LayoutHelpers.AtLeftTopIn(t, body, x, y)
        t:DisableHitTest()
        return t
    end
    local function rect(color, x, y, rw, rh)
        local b = Bitmap(body)
        b:SetSolidColor(color)
        LayoutHelpers.SetDimensions(b, rw, rh)
        LayoutHelpers.AtLeftTopIn(b, body, x, y)
        b:DisableHitTest()
        return b
    end
    text('Ping: row measures column', 14, 'ffe0e0e0', PAD, PAD)
    local close = text('Ctrl+Shift+P closes', 11, 'ff808080', 0, PAD + 3)
    LayoutHelpers.AtLeftTopIn(close, body, w - PAD - math.ceil(close.TextAdvance()), PAD + 3)
    grid.readout = text('', 11, 'ffe0e0e0', PAD, h - PAD - 84)
    grid.status = text('', 11, 'ffc0c0c0', PAD, h - PAD - 42)
    grid.maxLine = text('', 11, 'ffc0c0c0', PAD, h - PAD - 28)
    grid.delayLine = text('', 11, 'ffc0c0c0', PAD, h - PAD - 14)
    if n == 0 then
        body:DisableHitTest()
        text('Not in a networked lobby or game', 12, 'ffa0a0a0', PAD, PAD + 26)
        return
    end
    local gridLeft, gridTop = PAD + p.nameW, PAD + TITLE_H + p.headerH
    grid.origin = {gridLeft, gridTop}
    -- Column names, turned to read bottom to top, ending at the grid.
    for j, to in data.players do
        local t = text(names[j], p.font, to.id == data['local'] and 'ff7fb6ff' or 'ffd0d0d0', 0, 0)
        t:SetVertical(true)
        local lineH = t.FontAscent() + t.FontDescent()
        local x = gridLeft + (j - 1) * cell + math.floor((cell - lineH) / 2)
        LayoutHelpers.AtLeftTopIn(t, body, x, gridTop - 4 - math.ceil(t.TextAdvance()))
    end
    -- Every other row and column a shade lighter.
    for i = 2, n, 2 do
        rect('18ffffff', PAD, gridTop + (i - 1) * cell, p.nameW + n * cell, cell)
        rect('18ffffff', gridLeft + (i - 1) * cell, gridTop, cell, n * cell)
    end
    grid.rowBand = rect('30ffffff', PAD, gridTop, p.nameW + n * cell, cell)
    grid.colBand = rect('30ffffff', gridLeft, gridTop - p.headerH, cell, p.headerH + n * cell)
    grid.rowBand:Hide()
    grid.colBand:Hide()
    -- The input delay column, right of the grid.
    local delayX = gridLeft + n * cell + 8
    local head = text('input delay', p.font, 'ffa0a0a0', delayX, 0)
    LayoutHelpers.AtTopIn(head, body, gridTop - 4 - math.ceil(head.FontAscent() + head.FontDescent()))
    -- Each machine's top game speed, right of that.
    local speedX = delayX + p.delayW
    local speedHead = text('top speed', p.font, 'ffa0a0a0', speedX, 0)
    LayoutHelpers.AtTopIn(speedHead, body, gridTop - 4 - math.ceil(speedHead.FontAscent() + speedHead.FontDescent()))
    grid.delays, grid.speeds = {}, {}
    for i, from in data.players do
        local y = gridTop + (i - 1) * cell
        local d = text('', p.font, 'ffd0d0d0', delayX, y)
        LayoutHelpers.AtTopIn(d, body, y + math.floor((cell - d.FontAscent() - d.FontDescent()) / 2))
        grid.delays[from.id] = d
        local sp = text('', p.font, 'ffd0d0d0', speedX, y)
        LayoutHelpers.AtTopIn(sp, body, y + math.floor((cell - sp.FontAscent() - sp.FontDescent()) / 2))
        grid.speeds[from.id] = sp
        local name = text(names[i], p.font, from.id == data['local'] and 'ff7fb6ff' or 'ffd0d0d0', PAD, y)
        LayoutHelpers.AtTopIn(name, body, y + math.floor((cell - name.FontAscent() - name.FontDescent()) / 2))
        grid.cells[from.id] = {}
        for j, to in data.players do
            local x = gridLeft + (j - 1) * cell
            grid.cells[from.id][to.id] = rect(from.id == to.id and 'ff202020' or 'ff404040', x, y, cell - 1, cell - 1)
        end
    end
    text('Green < 80 ms, yellow < 150, red above    hover a cell for its ping, 99th and route', 11, 'ff909090', PAD, h - PAD - 70)
    text("Own cell: that machine's top game speed (purple: it limits the game speed)", 11, 'ff909090', PAD, h - PAD - 56)
    grid.players = data.players
    -- The cell under the mouse: its row and column lit, its link spelt out.
    body.HandleEvent = function(self, event)
        if event.Type == 'MouseMotion' then
            local x = (event.MouseX - self.Left()) / scale - gridLeft
            local y = (event.MouseY - self.Top()) / scale - gridTop
            local j, i = math.floor(x / cell) + 1, math.floor(y / cell) + 1
            if x >= 0 and y >= 0 and i >= 1 and j >= 1 and i <= n and j <= n then
                grid.hover = {i, j}
                LayoutHelpers.AtTopIn(grid.rowBand, body, gridTop + (i - 1) * cell)
                LayoutHelpers.AtLeftIn(grid.colBand, body, gridLeft + (j - 1) * cell)
                grid.rowBand:Show()
                grid.colBand:Show()
            else
                grid.hover = false
                grid.rowBand:Hide()
                grid.colBand:Hide()
            end
            grid.showReadout()
        elseif event.Type == 'MouseExit' then
            grid.hover = false
            grid.rowBand:Hide()
            grid.colBand:Hide()
            grid.showReadout()
        end
        return false
    end
end

-- False once the window is gone (closed, or the interface it was on was
-- torn down: lobby to game).
local function refresh()
    if grid.body and IsDestroyed(grid.body) then return false end
    local data = __net_ping_grid()
    local sig = data and signature(data) or ''
    if sig ~= grid.sig then
        grid.sig = sig
        build(data)
    end
    grid.data = data
    if not data then return true end
    local asked, now, low = speeds(data)
    for _, from in data.players do
        local sp = grid.speeds and grid.speeds[from.id]
        if sp and not IsDestroyed(sp) then
            sp:SetText(from.max and (string.gsub(string.format('%.2f', 10 ^ (from.max / 10)), '%.?0+$', '') .. 'x') or '-')
            -- Purple: the machine that limits the game's speed.
            sp:SetColor(from.max and from.max == low and 'ffc050ff' or 'ffd0d0d0')
        end
        local d = grid.delays and grid.delays[from.id]
        if d and not IsDestroyed(d) then
            d:SetText(delayText(from.delay))
            -- Half a second or more is felt.
            d:SetColor(from.delay and from.delay * 100 / rateNow() >= 500 and 'ffffc726' or 'ffd0d0d0')
        end
        local row = data.rows[from.id] or {}
        for _, to in data.players do
            local cell = grid.cells[from.id] and grid.cells[from.id][to.id]
            local e = row[to.id]
            if cell and from.id ~= to.id then
                cell:SetSolidColor(e and colorFor(e.ms) or 'ff404040')
            elseif cell then
                -- Own cell: unknown (lobby), the limit, or keeping up.
                local holds = from.max and from.max == low and low < asked
                cell:SetSolidColor((not from.max and 'ff202020') or (holds and 'ffc050ff') or 'ff3a6f9a')
            end
        end
    end
    -- Input delay: each player's own (it only delays their orders).
    if grid.delayLine and not IsDestroyed(grid.delayLine) then
        local mine, lo, hi, who
        for _, p in data.players do
            if p.delay then
                if p.id == data['local'] then mine = p.delay end
                if not lo or p.delay < lo then lo = p.delay end
                if not hi or p.delay > hi then hi, who = p.delay, p.name end
            end
        end
        if mine then
            local range = lo == hi and 'everyone the same' or string.format('players %s to %s (%s)', delayText(lo), delayText(hi), who)
            grid.delayLine:SetText(string.format('Input delay: yours %s; %s', delayText(mine), range))
        else
            grid.delayLine:SetText('')
        end
    end
    if grid.status and not IsDestroyed(grid.status) then
        if asked then
            local _, _, _, who = speeds(data)
            grid.status:SetText(string.format('Game speed: asked %s, now %s', steps(asked), steps(now)))
            -- The fastest the game can go: the slowest machine's top speed.
            grid.maxLine:SetText(string.format('Fastest possible: %s (limited by %s)', steps(low), who))
        else
            grid.status:SetText('')
            grid.maxLine:SetText('')
        end
    end
    grid.showReadout()
    return true
end

local function showReadout()
    if not grid or not grid.readout or IsDestroyed(grid.readout) then return end
    local data, hover = grid.data, grid.hover
    if not data or not hover then
        grid.readout:SetText('')
        return
    end
    local from, to = data.players[hover[1]], data.players[hover[2]]
    if not from or not to then return end
    if from.id == to.id then
        if from.max then
            local delay = from.delay and string.format('; orders run %d ticks (%s) after they are given', from.delay, delayText(from.delay)) or ''
            grid.readout:SetText(string.format('%s: keeps up with game speeds to %s%s', from.name, steps(from.max), delay))
        else
            grid.readout:SetText(from.name .. ': top game speed measured once the game runs')
        end
        return
    end
    local e = (data.rows[from.id] or {})[to.id]
    if not e then
        grid.readout:SetText(from.name .. ' -> ' .. to.name .. ': not measured yet')
        return
    end
    local path = (e.path == 'relay' and 'iroh relay') or (e.path == 'direct' and 'iroh direct') or 'LAN / IP'
    grid.readout:SetText(string.format('%s -> %s: %s ms, 99th %s ms, %s', from.name, to.name, ms(e.ms), ms(e.p99 or e.ms), path))
end

function __engine_toggle_ping_grid()
    if grid then
        grid.closed = true
        if grid.body and not IsDestroyed(grid.body) then grid.body:Destroy() end
        grid = false
        return
    end
    grid = { root = GetFrame(0), sig = nil, cells = {}, showReadout = showReadout }
    local mine = grid
    local ok, err = pcall(refresh)
    if not ok then WARN('ping grid: ' .. tostring(err)) end
    ForkThread(function()
        while not mine.closed do
            WaitSeconds(0.5)
            if mine.closed then break end
            local ok, alive = pcall(refresh)
            if not ok then WARN('ping grid: ' .. tostring(alive)) end
            if not ok or not alive then
                if grid == mine then grid = false end
                break
            end
        end
    end)
end
