-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
-- The Fatboy (UEF experimental mobile factory) surrounded by unarmed,
-- invulnerable dummy targets so every weapon engages: engineers on the four
-- diagonals (so each side turret must turn ~45 degrees off its rest facing)
-- inside the riot guns' 45 range (and the artillery turrets' 100), and
-- a scout circling inside the AA guns' 55. Each weapon's projectile creation
-- is counted and every one must fire (the torpedo is left out: no naval
-- target on land).
-- Visual: cargo run -- --script tests/scenarios/fatboy.lua --camera 420,606,70
ForkThread(function()
    WaitTicks(2)
    local cx, cz = 420, 600
    local fatboy = CreateUnitHPR('uel0401', 'ARMY_1', cx, ground(cx, cz), cz, 0, 0, 0)
    IssueStop({fatboy})
    local dummies = {}
    for _, o in {{21, 21}, {-21, 21}, {21, -21}, {-21, -21}} do
        local x, z = cx + o[1], cz + o[2]
        local d = CreateUnitHPR('uel0105', 'ARMY_2', x, ground(x, z), z, 0, 0, 0)
        d:SetCanTakeDamage(false)
        table.insert(dummies, d)
    end
    local scout = CreateUnitHPR('uea0101', 'ARMY_2', cx + 25, ground(cx + 25, cz), cz, 0, 0, 0)
    scout:SetCanTakeDamage(false)
    table.insert(dummies, scout)

    -- Count shots per weapon (FAF's projectile weapons all fire through
    -- CreateProjectileAtMuzzle).
    local labels = {}
    local fired = {}
    for i = 1, fatboy:GetWeaponCount() do
        local w = fatboy:GetWeapon(i)
        local label = w.Blueprint.Label
        if label ~= 'DeathWeapon' and label ~= 'Torpedo' then
            table.insert(labels, label)
            fired[label] = 0
            local create = w.CreateProjectileAtMuzzle
            w.CreateProjectileAtMuzzle = function(self, muzzle)
                fired[label] = fired[label] + 1
                return create(self, muzzle)
            end
        end
    end
    check(table.getn(labels) >= 8, 'the Fatboy has its 8 non-torpedo weapons (got ' .. table.getn(labels) .. ')')

    -- Keep the scout circling the Fatboy within AA range.
    ForkThread(function()
        local a = 0
        while not scout.Dead do
            a = a + 1
            local x, z = cx + 30 * math.cos(a), cz + 30 * math.sin(a)
            IssueClearCommands({scout})
            IssueMove({scout}, {x, ground(x, z), z})
            WaitTicks(20)
        end
    end)

    local function all_fired()
        for _, l in labels do
            if fired[l] == 0 then return false end
        end
        return true
    end
    for _ = 1, 600 do
        WaitTicks(1)
        if all_fired() then break end
    end
    for _, l in labels do
        check(fired[l] > 0, l .. ' fired')
        note(l .. ' shots: ' .. fired[l])
    end
    for _, d in dummies do
        check(not d.Dead, 'the invulnerable dummies survive')
    end
    note('all weapons engaged by tick ' .. GetGameTick())
    done()
end)
