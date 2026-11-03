-- Probe hook, mounted at /schook by run_faf.ps1: once the game interface is
-- made, runs the probe map's interface half (ui_probe.lua beside its scenario)
-- when it has one, so a probe can log what the user layer sees.
local ProbeCreateUI = CreateUI
function CreateUI(isReplay)
    ProbeCreateUI(isReplay)
    local file = string.gsub(SessionGetScenarioInfo().file, '[^/]*$', '') .. 'ui_probe.lua'
    if DiskGetFileInfo(file) then
        import(file)
    end
end
