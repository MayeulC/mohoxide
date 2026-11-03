-- Land, air and naval factories of each faction building a unit (roll-off).
-- cargo run -- --no-ai --script dev/scenes/visual/factory_rolloff.lua --camera 420,600,40
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local function at(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local jobs = { { 'ueb0101', 'uel0201' }, { 'urb0101', 'url0107' }, { 'uab0101', 'ual0201' }, { 'xsb0101', 'xsl0201' } }
    for i, j in jobs do
        local f = at(j[1], 395 + i * 14, 600)
        IssueBuildFactory({ f }, j[2], 1)
    end
    while true do
        WaitTicks(1)
        for _, u in b:GetListOfUnits(categories.MOBILE, false) or {} do
            local p = u:GetPosition()
            if u:GetFractionComplete() >= 1 then
                LOG('ROLLOFF tick ', GetGameTick(), ' ', u.UnitId, string.format(' %.2f %.2f %.2f ground %.2f', p[1], p[2], p[3], GetTerrainHeight(p[1], p[3])))
            end
        end
    end
end)
