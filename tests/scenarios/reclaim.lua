-- Scenario test (tests/scenarios.rs): ARMY_1 / ARMY_2 have no AI; checks go
-- into ScenarioResult, which the test reads once `done` is set.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
local function done() ScenarioResult.done = true end
local function ground(x, z) return GetTerrainHeight(x, z) end
local function rich(army)
    local b = ArmyBrains[army]
    b:__engine_SetStorage("MASS", 50000) b:__engine_SetStorage("ENERGY", 500000)
    b:GiveResource("MASS", 50000) b:GiveResource("ENERGY", 500000)
    return b
end
-- An engineer reclaims a rock: the prop goes, the army gains its mass, and
-- the engineer's work bar runs from 0 to a rising fraction, clears when the
-- reclaim is interrupted, comes back seeded with the taken share when the
-- same rock is taken up again, and clears when done.
ForkThread(function()
    WaitTicks(2)
    local brain = ArmyBrains[1]
    brain:__engine_SetStorage('MASS', 1000)
    local prop = CreateProp({410, ground(410, 600), 600}, '/env/evergreen/props/rocks/fieldstone01_prop.bp')
    prop:SetMaxReclaimValues(1, 40, 10)
    local eng = CreateUnitHPR('uel0105', 'ARMY_1', 404, ground(404, 600), 600, 0, 0, 0)
    local before = brain:GetEconomyStored('MASS')
    check(eng:GetWorkProgress() == 0, 'no work bar before reclaiming')
    IssueReclaim({eng}, prop)
    -- The rock's 40 mass at the engineer's build rate 5 takes 0.8 s
    -- (Prop:GetReclaimCosts: TimeReclaim * max / rate / 10): 1/8 of the bar
    -- each tick once in reach, so samples every 2 ticks (`WaitTicks(n)` waits
    -- n - 1) crest 0.5 before it completes.
    local maxBar = 0
    for _ = 1, 60 do
        WaitTicks(3)
        maxBar = math.max(maxBar, eng:GetWorkProgress())
        if maxBar > 0.25 or prop:BeenDestroyed() then break end
    end
    check(maxBar >= 0.4, 'the work bar rose while reclaiming (max ' .. maxBar .. ')')
    local taken = brain:GetEconomyStored('MASS') - before
    check(taken > 10 and taken < 30, 'the bar matched the mass taken partway: ' .. taken)
    -- Interrupt (Issue* orders queue, so clear the command), then walk off
    -- with a move and take the same partly taken rock again: the bar clears
    -- and comes back seeded with the taken share.
    IssueClearCommands({eng})
    IssueMove({eng}, {400, ground(400, 600), 600})
    WaitTicks(3)
    check(eng:GetWorkProgress() == 0, 'the work bar cleared when the order changed')
    check(not prop:BeenDestroyed(), 'the rock kept what was already taken')
    IssueReclaim({eng}, prop)
    local seeded = 0
    for _ = 1, 50 do
        WaitTicks(2)
        seeded = eng:GetWorkProgress()
        if seeded > 0 or prop:BeenDestroyed() then break end
    end
    check(seeded > 0, 'the resumed reclaim showed the taken share, got ' .. seeded)
    check(math.abs(seeded - taken / 40) < 0.3, 'the resumed bar seeded at the taken share (' .. taken / 40 .. '), got ' .. seeded)
    for _ = 1, 100 do
        WaitTicks(2)
        if prop:BeenDestroyed() then break end
    end
    WaitTicks(2)
    check(prop:BeenDestroyed(), 'the rock was reclaimed')
    local gained = brain:GetEconomyStored('MASS') - before
    check(gained > 39 and gained < 41.5, 'gained the rock\'s 40 mass, got ' .. gained)
    check(eng:GetWorkProgress() == 0, 'the work bar cleared when done')
    -- Two engineers on one rock: both show a bar, and it clears for both.
    -- The rock is big enough that the slower engineer (12 studs off) gets
    -- there while eng1 is only partway through.
    brain:__engine_SetStorage('MASS', 10000)
    brain:GiveResource('MASS', 10)
    WaitTicks(2) -- a gift is stored on the next economy tick
    local before2 = brain:GetEconomyStored('MASS')
    local rock2 = CreateProp({410, ground(410, 600), 600}, '/env/evergreen/props/rocks/fieldstone01_prop.bp')
    rock2:SetMaxReclaimValues(1, 600, 100)
    local eng2 = CreateUnitHPR('uel0105', 'ARMY_1', 398, ground(398, 600), 600, 0, 0, 0)
    IssueReclaim({eng, eng2}, rock2)
    local eng1Bar, eng2Bar = false, false
    for _ = 1, 200 do
        WaitTicks(2)
        eng1Bar = eng1Bar or eng:GetWorkProgress() > 0
        eng2Bar = eng2Bar or eng2:GetWorkProgress() > 0
        if rock2:BeenDestroyed() then break end
    end
    WaitTicks(2)
    check(rock2:BeenDestroyed(), 'the shared rock was reclaimed')
    check(eng1Bar and eng2Bar, 'both reclaimers showed a work bar')
    check(eng:GetWorkProgress() == 0 and eng2:GetWorkProgress() == 0, 'both work bars cleared when done')
    local gained2 = brain:GetEconomyStored('MASS') - before2
    check(gained2 > 599 and gained2 < 603, 'gained the shared rock\'s 600 mass once, got ' .. gained2)
    done()
end)
