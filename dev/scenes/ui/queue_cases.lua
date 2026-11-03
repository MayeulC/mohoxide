-- Construction-panel queues that aren't a factory's: an ACU and a
-- Fatboy (mobile factory: its queue is its external factory's) side by side.
-- The Fatboy gets twenty tanks and five engineers queued here; the ACU's
-- enhancements are queued from the interface (FAF keeps the enhancement queue
-- there), e.g.:
-- cargo run -- --no-ai --player 1 --script dev/scenes/ui/queue_cases.lua --camera 420,600,60 --select <ACU id> \
--   --ui-lua "import('/lua/ui/notify/enhancementqueue.lua') ..."
-- Ids are logged as QUEUECASE lines.
ForkThread(function()
    WaitTicks(2)
    local b = ArmyBrains[1]
    b:__engine_SetStorage('MASS', 50000) b:__engine_SetStorage('ENERGY', 500000)
    b:GiveResource('MASS', 50000) b:GiveResource('ENERGY', 500000)
    local function at(bp, x, z) return CreateUnitHPR(bp, 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local acu = at('uel0001', 410, 600)
    local fatboy = at('uel0401', 440, 600)
    LOG('QUEUECASE acu ', acu.EntityId, ' fatboy ', fatboy.EntityId)
    WaitTicks(2)
    if fatboy.ExternalFactory then
        LOG('QUEUECASE exfac ', fatboy.ExternalFactory.EntityId)
        IssueBuildFactory({ fatboy.ExternalFactory }, 'uel0201', 20)
        IssueBuildFactory({ fatboy.ExternalFactory }, 'uel0105', 5)
    end
end)
