-- Frostmill Ruins' save with 64 armies: ARMY_1..64 on an 8 x 8 grid of
-- start markers (512 apart, from 256), each army a copy of ARMY_1's entry.
doscript('/maps/SCMP_030/scmp_030_save.lua', ScenarioInfo.Env)
local markers = Scenario.MasterChain._MASTERCHAIN_.Markers
local template = Scenario.Armies.ARMY_1
local armies = {}
for i = 1, 64 do
    local col, row = math.mod(i - 1, 8), math.floor((i - 1) / 8)
    local x, z = 256 + col * 512, 256 + row * 512
    markers['ARMY_' .. i] = {
        ['color'] = STRING('ff800080'),
        ['type'] = STRING('Blank Marker'),
        ['prop'] = STRING('/env/common/props/markers/M_Blank_prop.bp'),
        ['orientation'] = VECTOR3(0, 0, 0),
        ['position'] = VECTOR3(x, 0, z),
    }
    armies['ARMY_' .. i] = table.deepcopy(template)
end
armies.NEUTRAL_CIVILIAN = Scenario.Armies.NEUTRAL_CIVILIAN
Scenario.Armies = armies
