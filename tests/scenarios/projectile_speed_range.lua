-- Scenario test (tests/scenarios.rs): projectiles made by CreateProjectile
-- each draw their own speed in InitialSpeed +- InitialSpeedRange
-- (SDFOhCannon03: 14.6 +- 3).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
ForkThread(function()
    WaitTicks(2)
    local tank = CreateUnitHPR('uel0201', 'ARMY_1', 400, GetTerrainHeight(400, 570), 570, 0, 0, 0)
    local speeds, distinct, inside = {}, {}, true
    for i = 1, 8 do
        local p = tank:CreateProjectile('/projectiles/SDFOhCannon03/SDFOhCannon03_proj.bp', 0, 3, 0, 0, 0, 1)
        local speed = p:GetCurrentSpeed() * 10
        speeds[i] = string.format('%.2f', speed)
        distinct[speeds[i]] = true
        inside = inside and speed >= 11.6 - 1e-3 and speed <= 17.6 + 1e-3
    end
    local count = 0
    for _ in pairs(distinct) do count = count + 1 end
    note('speeds ' .. table.concat(speeds, ' '))
    check(count > 1, 'speeds vary (all ' .. speeds[1] .. ')')
    check(inside, 'within 14.6 +- 3')
    done()
end)
