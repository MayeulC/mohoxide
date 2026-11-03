-- Scenario test (tests/scenarios.rs): an enemy unit driving into an army's
-- vision gets OnDetectedBy(army) once, and that army's brain hears
-- OnIntelChange(blip, 'LOSNow', true), then false when it drives out.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    WaitTicks(2)
    local watcher = CreateUnitHPR('uel0101', 'ARMY_1', 400, GetTerrainHeight(400, 640), 640, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_2', 445, GetTerrainHeight(445, 640), 640, 0, 0, 0)
    local detected = {}
    tank.OnDetectedBy = function(self, army) table.insert(detected, army) end
    local changes = {}
    local brain = ArmyBrains[1]
    local orig = brain.OnIntelChange
    brain.OnIntelChange = function(self, blip, recon, val)
        if blip:GetSource() == tank and recon == 'LOSNow' then
            table.insert(changes, tostring(val))
        end
        return orig(self, blip, recon, val)
    end
    WaitTicks(5)
    check(table.getn(changes) == 0, 'not seen while far away')
    IssueMove({tank}, { 412, GetTerrainHeight(412, 640), 640 })
    for _ = 1, 300 do
        WaitTicks(1)
        if changes[1] then break end
    end
    check(changes[1] == 'true', 'LOSNow gained: ' .. table.concat(changes, ' '))
    check(table.getn(detected) == 1 and detected[1] == 1, 'OnDetectedBy(1) once: ' .. table.concat(detected, ' '))
    IssueClearCommands({tank})
    IssueMove({tank}, { 450, GetTerrainHeight(450, 640), 640 })
    for _ = 1, 400 do
        WaitTicks(1)
        if changes[2] then break end
    end
    check(changes[2] == 'false', 'LOSNow lost: ' .. table.concat(changes, ' '))
    IssueClearCommands({tank})
    IssueMove({tank}, { 412, GetTerrainHeight(412, 640), 640 })
    for _ = 1, 400 do
        WaitTicks(1)
        if changes[3] then break end
    end
    check(changes[3] == 'true', 'seen again: ' .. table.concat(changes, ' '))
    check(table.getn(detected) == 1, 'OnDetectedBy only the first time: ' .. table.concat(detected, ' '))
    ScenarioResult.done = true
end)
