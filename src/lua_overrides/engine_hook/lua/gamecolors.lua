
-- Engine hook: FAF has 19 player colours; the lobby takes up to 63 players
-- (`lobbycomm.lua`'s engine hook), so 44 more follow FAF's, in four bands of
-- hues at different brightness. Army and player colours stay in step, and
-- the lobby hands them out in this order after FAF's own.
do
    local MoreColors = {
        "ffff7373", -- (20)
        "ffffbf73", -- (21)
        "fff2ff73", -- (22)
        "ffa6ff73", -- (23)
        "ff73ff8c", -- (24)
        "ff73ffd9", -- (25)
        "ff73d9ff", -- (26)
        "ff738cff", -- (27)
        "ffa673ff", -- (28)
        "fff273ff", -- (29)
        "ffff73bf", -- (30)
        "ff991500", -- (31)
        "ff996800", -- (32)
        "ff769900", -- (33)
        "ff239900", -- (34)
        "ff009931", -- (35)
        "ff009984", -- (36)
        "ff005a99", -- (37)
        "ff000799", -- (38)
        "ff4c0099", -- (39)
        "ff990092", -- (40)
        "ff99003f", -- (41)
        "ffd9a28d", -- (42)
        "ffd9cb8d", -- (43)
        "ffbdd98d", -- (44)
        "ff94d98d", -- (45)
        "ff8dd9af", -- (46)
        "ff8dd9d9", -- (47)
        "ff8dafd9", -- (48)
        "ff948dd9", -- (49)
        "ffbd8dd9", -- (50)
        "ffd98dcb", -- (51)
        "ffd98da2", -- (52)
        "ff61300f", -- (53)
        "ff615d0f", -- (54)
        "ff38610f", -- (55)
        "ff0f6112", -- (56)
        "ff0f613f", -- (57)
        "ff0f5661", -- (58)
        "ff0f2961", -- (59)
        "ff210f61", -- (60)
        "ff4e0f61", -- (61)
        "ff610f47", -- (62)
        "ff610f1a", -- (63)
    }
    for _, color in MoreColors do
        table.insert(GameColors.ArmyColors, color)
        table.insert(GameColors.PlayerColors, color)
        table.insert(GameColors.LobbyColorOrder, table.getn(GameColors.PlayerColors))
    end
end
