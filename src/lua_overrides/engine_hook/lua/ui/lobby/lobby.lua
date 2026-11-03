
-- Engine hook: FAF lays its slot rows one below the other, room for 16.
-- With more slots (`lobbycomm.lua`'s engine hook) the rows show as many as
-- fit above the chat box at a time and scroll (wheel over the rows, or the
-- scrollbar) through the map's start spots; at 16 or fewer it is FAF's
-- layout unchanged.
do
    local FafSlotRows = 16
    local CreateSlotsUIBeforeScrolling = CreateSlotsUI
    local UpdateAvailableSlotsBeforeScrolling = UpdateAvailableSlots

    local function SlotRowsScrollable()
        return LobbyComm.maxPlayerSlots > FafSlotRows
    end

    -- The rows that fit between the column labels and the chat box (FAF's
    -- 16th row runs into the chat box's frame).
    local function VisibleSlotRows()
        local rowStep = GUI.slots[1].Height() + LayoutHelpers.ScaleNumber(3)
        local space = GUI.chatPanel.Top() - LayoutHelpers.ScaleNumber(6) - GUI.labelGroup.Bottom()
        return math.max(1, math.min(FafSlotRows, math.floor(space / rowStep)))
    end

    -- Lays out the rows from `GUI.slotRows.top`: those in view below the
    -- column labels, each shown as FAF last asked; the others hidden with
    -- their backgrounds.
    local function LayOutSlotRows()
        local rows = GUI.slotRows
        local slots = GUI.slots
        local visibleRows = VisibleSlotRows()
        rows.top = math.max(1, math.min(rows.top, numOpenSlots - visibleRows + 1))
        local bottom = rows.top + visibleRows - 1
        for index = 1, LobbyComm.maxPlayerSlots do
            local slot = slots[index]
            if index >= rows.top and index <= bottom then
                if index == rows.top then
                    LayoutHelpers.Below(slot, GUI.labelGroup)
                else
                    LayoutHelpers.Below(slot, slots[index - 1], 3)
                end
                slot.SlotBackground:Show()
                if slot.ShownByLobby then
                    slot.ShowInView(slot)
                else
                    slot.HideInView(slot)
                end
            else
                slot.HideInView(slot)
                slot.SlotBackground:Hide()
            end
        end
        if rows.scrollbar then
            rows.scrollbar:SetHidden(numOpenSlots <= visibleRows)
        end
    end

    function CreateSlotsUI(makeLabel)
        CreateSlotsUIBeforeScrolling(makeLabel)
        if not SlotRowsScrollable() then
            return
        end
        local slots = GUI.slots
        -- The area the rows in view cover: the scrollbar's frame.
        local rows = Group(GUI.playerPanel, "slotRows")
        rows.top = 1
        LayoutHelpers.AtLeftTopIn(rows, GUI.labelGroup, 0, 0)
        rows.Top:Set(function() return GUI.labelGroup.Bottom() end)
        rows.Right:Set(function() return slots[1].Right() end)
        rows.Height:Set(function() return VisibleSlotRows() * (slots[1].Height() + LayoutHelpers.ScaleNumber(3)) end)
        rows:DisableHitTest()
        GUI.slotRows = rows

        for index = 1, LobbyComm.maxPlayerSlots do
            local slot = slots[index]
            -- FAF shows and hides rows as start spots open and close; the
            -- row only appears when it is also in view.
            slot.ShowInView, slot.HideInView = slot.Show, slot.Hide
            slot.ShownByLobby = not slot:IsHidden()
            slot.Show = function(self)
                self.ShownByLobby = true
                if index >= rows.top and index < rows.top + VisibleSlotRows() then
                    self.ShowInView(self)
                end
            end
            slot.Hide = function(self)
                self.ShownByLobby = false
                self.HideInView(self)
            end
        end

        -- The scrollbar drives `rows`; the wheel anywhere over the rows
        -- reaches the player panel (their parent).
        rows.GetScrollValues = function(self, axis)
            return 1, numOpenSlots, self.top, math.min(self.top + VisibleSlotRows() - 1, numOpenSlots)
        end
        rows.ScrollLines = function(self, axis, delta)
            self:ScrollSetTop(axis, self.top + math.floor(delta))
        end
        rows.ScrollPages = function(self, axis, delta)
            self:ScrollSetTop(axis, self.top + math.floor(delta) * VisibleSlotRows())
        end
        rows.ScrollSetTop = function(self, axis, top)
            self.top = math.floor(top)
            LayOutSlotRows()
        end
        rows.IsScrollable = function(self, axis)
            return numOpenSlots > VisibleSlotRows()
        end
        GUI.playerPanel.HandleEvent = function(self, event)
            if event.Type == 'WheelRotation' and rows:IsScrollable() then
                rows:ScrollLines(nil, event.WheelRotation > 0 and -1 or 1)
                return true
            end
            return false
        end
        rows.scrollbar = UIUtil.CreateLobbyVertScrollbar(rows, 0)
        LayOutSlotRows()
    end

    function UpdateAvailableSlots(numAvailStartSpots, scenario)
        UpdateAvailableSlotsBeforeScrolling(numAvailStartSpots, scenario)
        if GUI.slotRows then
            LayOutSlotRows()
        end
    end
end

-- Engine hook: FAF's lobby lets a game launch when everyone has the game's
-- mods by uid. Every peer must run the same files, or the game desyncs:
-- the host's launch also waits until each player's copy of each sim mod
-- matches its own (the engine hashes every installed mod's files).
do
    local TryLaunchWithoutModCheck = TryLaunch
    TryLaunch = function(skipNoObserversCheck)
        if lobbyComm and lobbyComm:IsHost() and gameInfo then
            local problems = __net_mod_problems(gameInfo.GameMods)
            if problems[1] then
                for _, problem in problems do
                    AddChatText(LOCF("<LOC engine_mods_0001>Can't launch: %s", problem))
                end
                return
            end
        end
        return TryLaunchWithoutModCheck(skipNoObserversCheck)
    end
end
