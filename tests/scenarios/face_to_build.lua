-- Scenario test (tests/scenarios.rs): a Mantis (NeedToFaceTargetToBuild)
-- with its back to a damaged tank turns to face it before it repairs.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
end
local function heading(u) local o = u:GetOrientation() return math.deg(2 * math.atan2(o[2], o[4])) end
ForkThread(function()
    WaitTicks(2)
    rich(1)
    local mantis = CreateUnitHPR('url0107', 'ARMY_1', 400, ground(400, 590), 590, 0, 0, 0)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, ground(400, 584), 584, 0, 0, 0)
    tank:SetHealth(tank, tank:GetMaxHealth() / 2)
    local hp0, h0 = tank:GetHealth(), heading(mantis)
    IssueRepair({mantis}, tank)
    WaitTicks(4)
    local early = tank:GetHealth()
    local repairing, turned = false, false
    for _ = 1, 40 do
        WaitTicks(2)
        if tank:GetHealth() > hp0 and not repairing then
            repairing = true
            turned = math.abs(math.abs(heading(mantis) - h0) - 180) < 20
        end
    end
    note('repairing ' .. tostring(repairing) .. ' turned ' .. tostring(turned))
    note(string.format('heading %.0f -> %.0f, hp %.0f -> %.0f -> %.0f', h0, heading(mantis), hp0, early, tank:GetHealth()))
    check(early == hp0, 'no repair while facing away (hp ' .. early .. ')')
    check(repairing, 'then it repairs')
    check(turned, 'having turned to face the tank')
    done()
end)
