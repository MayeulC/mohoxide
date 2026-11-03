-- Probe: what moho puts in the order table FAF's commandmode.OnCommandIssued
-- gets (Blueprint, Target) for a guard, an attack and a move. The orders
-- need a person: FAF's Lua can't give a targeted order, so right-click the
-- extractor (guard), the tank (attack) and the ground (move) with the
-- engineer the interface half selects (ui_probe.lua logs each order).
--   real game (Windows): dev/probes/run_faf.ps1 probe_guardcmd 300
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local start = ScenarioUtils.GetMarker('ARMY_1').position
local best, bestDist
for _, m in ScenarioUtils.GetMarkers() do
    if m.type == 'Mass' then
        local d = VDist2(m.position[1], m.position[3], start[1], start[3])
        if not bestDist or d < bestDist then best, bestDist = m.position, d end
    end
end
local function make(bp, army, x, z)
    local u = CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
    u:SetCanTakeDamage(false)
    return u
end
local armies = ListArmies()
local mex = make('ueb1202', armies[1], best[1], best[3])
local eng = make('uel0105', armies[1], best[1] + 6, best[3])
local tank = make('uel0201', armies[2], best[1], best[3] + 12)
tank:SetWeaponEnabledByLabel('MainGun', false)
LOG('PROBE sim deposit ', best[1], ',', best[3], ' mex ', mex:GetEntityId(), ' at ', mex:GetPosition()[1], ',', mex:GetPosition()[3],
    ' engineer ', eng:GetEntityId(), ' tank ', tank:GetEntityId())
