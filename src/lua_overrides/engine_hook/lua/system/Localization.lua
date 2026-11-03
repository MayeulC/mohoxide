-- Engine hook (non-Windows only, see `LUA_OVERRIDES`): FAF's in-game tabs
-- label their exit with `_Exit_to_Windows`, and there is no Windows to exit
-- to. Give that key the locale's `_Exit`, as the main menu says,
-- rather than wrapping `LOC`: no other interface text changes.
--
-- Appended to FAF's `Localization.lua` (the hook folder), so `loc_table` and
-- `language` are that file's own. If a future FAF renames them, this does
-- nothing rather than breaking localization.
do
    if type(loc_table) == 'table' then
        local function useDesktopExit()
            if loc_table._Exit ~= nil then
                loc_table._Exit_to_Windows = loc_table._Exit
            end
        end
        useDesktopExit()
        if type(language) == 'function' then
            local baseLanguage = language
            function language(la)
                baseLanguage(la)
                useDesktopExit()
            end
        end
    end
end
