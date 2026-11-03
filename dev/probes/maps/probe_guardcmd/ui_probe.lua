-- Interface half of probe_guardcmd: selects the engineer, puts the camera
-- on it and logs every order moho hands commandmode.OnCommandIssued. Ends
-- (PROBE done) after four orders.
local commandmode = import('/lua/ui/game/commandmode.lua')
local original = commandmode.OnCommandIssued
local count = 0
commandmode.OnCommandIssued = function(command)
    count = count + 1
    local target = command.Target or {}
    local p = target.Position
    local targetBlueprint = target.EntityId and GetUnitById(target.EntityId) and GetUnitById(target.EntityId):GetBlueprint().BlueprintId
    LOG('PROBE cmd ', count, ' ', tostring(command.CommandType),
        ' Blueprint=[', tostring(command.Blueprint), '] (', type(command.Blueprint), ')',
        ' Target.Type=', tostring(target.Type), ' EntityId=', tostring(target.EntityId), ' (', tostring(targetBlueprint), ')',
        ' Position=', p and string.format('%.2f,%.2f,%.2f', p[1], p[2], p[3]) or 'nil',
        ' Units=', table.getn(command.Units or {}), ' Clear=', tostring(command.Clear))
    if count >= 4 then
        LOG('PROBE done')
    end
    return original(command)
end
ForkThread(function()
    WaitSeconds(3)
    UISelectionByCategory('ENGINEER', false, false, false, false)
    local units = GetSelectedUnits()
    if units and units[1] then
        GetCamera('WorldCamera'):MoveTo(units[1]:GetPosition(), {0, 1.2, 0}, 60, 1)
    end
    LOG('PROBE ready: right-click the extractor, the tank, the ground, then the extractor again')
end)
