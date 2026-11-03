-- Scenario test (tests/scenarios.rs): missile silos. A UEF tactical
-- missile launcher builds a missile for a silo build order
-- (OnSiloBuildStart / OnSiloBuildEnd, ammo 1), holds fire at an enemy in
-- range (manual fire), launches on an IssueTactical order (ammo 0), and in
-- auto mode (OnAutoModeOn) starts the next missile by itself. It starts
-- in auto mode (its blueprint's AI.InitialAutoMode), turned off here so
-- the ordered build can be checked first.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function make(bp, army, x, z, h) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, h or 0, 0) end
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    brain:__engine_SetStorage('MASS', 50000) brain:__engine_SetStorage('ENERGY', 500000)
    brain:GiveResource('MASS', 50000) brain:GiveResource('ENERGY', 500000)
    local tml = make('ueb2108', 'ARMY_1', 400.5, 600.5)
    check(tml:__engine_GetAutoMode(), 'it starts in auto mode (InitialAutoMode)')
    tml:SetAutoMode(false)
    local seen = {}
    for _, name in {'OnSiloBuildStart', 'OnSiloBuildEnd', 'OnAutoModeOn'} do
        local orig = tml[name]
        tml[name] = function(self, ...) table.insert(seen, name) return orig(self, ...) end
    end
    WaitTicks(2)
    IssueSiloBuildTactical({tml})
    WaitTicks(5)
    check(tml:IsUnitState('SiloBuildingAmmo'), 'it builds a missile for the order')
    for _ = 1, 400 do
        if tml:GetTacticalSiloAmmoCount() > 0 then break end
        WaitTicks(1)
    end
    local s = table.concat(seen, ' ')
    check(tml:GetTacticalSiloAmmoCount() == 1, 'one missile built: ' .. tml:GetTacticalSiloAmmoCount())
    check(s == 'OnSiloBuildStart OnSiloBuildEnd', 'start then end: ' .. s)
    check(not tml:IsUnitState('SiloBuildingAmmo'), 'and it stops building')
    local enemy = make('uel0201', 'ARMY_2', 440.5, 600.5)
    WaitTicks(30)
    check(tml:GetTacticalSiloAmmoCount() == 1, 'it holds fire at an enemy in range (manual fire)')
    IssueTactical({tml}, enemy:GetPosition())
    for _ = 1, 100 do
        if tml:GetTacticalSiloAmmoCount() == 0 then break end
        WaitTicks(1)
    end
    check(tml:GetTacticalSiloAmmoCount() == 0, 'it launches on a tactical order')
    tml:SetAutoMode(true)
    WaitTicks(5)
    check(string.find(table.concat(seen, ' '), 'OnAutoModeOn') ~= nil, 'auto mode tells the script')
    check(tml:IsUnitState('SiloBuildingAmmo'), 'and builds the next missile unordered')
    -- A nuke launcher with one missile launches it on a nuke order
    -- (OnNukeLaunched).
    local nuke = make('ueb2305', 'ARMY_1', 380.5, 620.5)
    WaitTicks(2)
    nuke:GiveNukeSiloAmmo(1)
    local launched = false
    local origL = nuke.OnNukeLaunched
    nuke.OnNukeLaunched = function(self, ...) launched = true return origL(self, ...) end
    IssueNuke({nuke}, {600, GetTerrainHeight(600, 600), 600})
    for _ = 1, 150 do
        if launched then break end
        WaitTicks(1)
    end
    check(launched, 'the nuke launches (OnNukeLaunched)')
    check(nuke:GetNukeSiloAmmoCount() == 0, 'using its missile')
    ScenarioResult.done = true
end)
