
-- Engine hook: the game interface listens for picture-in-picture requests
-- (`/lua/ui/game/pip.lua`), and a replay shows its timeline
-- (`/lua/ui/game/timeline.lua`).
do
    local Base = CreateUI
    function CreateUI(isReplay)
        Base(isReplay)
        import("/lua/ui/game/pip.lua").Init()
        import("/lua/ui/game/timeline.lua").Init()
    end
end
