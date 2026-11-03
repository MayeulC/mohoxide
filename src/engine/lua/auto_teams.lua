-- FAF's lobby AutoTeams for a game set up without the lobby (lobby.lua's
-- AssignAutoTeams): 'tvsb' puts armies starting in the map's top half on
-- team 2 and the rest on team 3, 'lvsr' the left half, 'pvsi' odd start
-- spots against even. Runs after SetupSession (the map's script may add
-- start markers) and before the brains are made; civilians keep theirs.
local layout = ...
local markers = Scenario.MasterChain._MASTERCHAIN_.Markers
local size = ScenarioInfo.size or { 512, 512 }
local spots = {}
for i, name in ScenarioInfo.Configurations.standard.teams[1].armies do spots[name] = i end
for name, setup in ScenarioInfo.ArmySetup do
    if not setup.Civilian then
        local team
        if layout == 'pvsi' then
            local spot = spots[name] or setup.StartSpot or 1
            team = math.mod(spot, 2) ~= 0 and 2 or 3
        else
            local marker = markers[name]
            if marker then
                local p = marker.position
                if layout == 'tvsb' then
                    team = p[3] < size[2] / 2 and 2 or 3
                else
                    team = p[1] < size[1] / 2 and 2 or 3
                end
            end
        end
        if team then setup.Team = team end
    end
end
