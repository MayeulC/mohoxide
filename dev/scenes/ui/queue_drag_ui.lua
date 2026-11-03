-- Interface half of dev/scenes/ui/queue_drag.lua (with --select <the factory>): drags
-- the selected factory's first queue entry onto its last as the mouse would
-- (MouseEnter, a left ButtonPress, MouseExit, MouseEnter on the target,
-- ButtonRelease: the events the engine delivers to the queue buttons) and
-- logs the queue before and after (QUEUEDRAG lines). QUEUEDRAG_NO_DRAG = true
-- first only logs (the before case).
ForkThread(function()
    local construction = import('/lua/ui/game/construction.lua')
    local function queueText()
        local u = GetSelectedUnits() and GetSelectedUnits()[1]
        local s = {}
        for _, e in (u and SetCurrentFactoryForQueueDisplay(u)) or {} do table.insert(s, e.id .. 'x' .. e.count) end
        return table.concat(s, ',')
    end
    local function queueButtons()
        local out = {}
        for _, b in (construction.controls.secondaryChoices and construction.controls.secondaryChoices.Items) or {} do
            if b.Data and b.Data.type == 'queuestack' then out[b.Data.position] = b end
        end
        return out
    end
    WaitSeconds(1)
    LOG('QUEUEDRAG option gui_draggable_queue = ', tostring(import('/lua/user/prefs.lua').GetFromCurrentProfile('options').gui_draggable_queue))
    LOG('QUEUEDRAG queue before: ', queueText())
    if rawget(_G, 'QUEUEDRAG_NO_DRAG') then return end
    local buttons = queueButtons()
    local n = table.getn(buttons)
    local from, to = buttons[1], buttons[n]
    if not (from and to and n > 1) then LOG('QUEUEDRAG no queue buttons') return end
    local mods = { Left = true }
    from:HandleEvent({ Type = 'MouseEnter', Modifiers = {} })
    from:HandleEvent({ Type = 'ButtonPress', Modifiers = mods })
    from:HandleEvent({ Type = 'MouseExit', Modifiers = mods })
    to:HandleEvent({ Type = 'MouseEnter', Modifiers = mods })
    to:HandleEvent({ Type = 'ButtonRelease', Modifiers = {} })
    to:HandleEvent({ Type = 'MouseExit', Modifiers = {} })
    LOG('QUEUEDRAG dragged entry 1 onto entry ', n)
    WaitSeconds(1)
    LOG('QUEUEDRAG queue after: ', queueText())
end)
