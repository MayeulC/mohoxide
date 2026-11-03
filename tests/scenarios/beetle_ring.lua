-- Scenario test (tests/scenarios.rs): the Fire Beetle's
-- ExplosionRadiusRingDummyWeapon, as FAF plays it (probe_dummies,
-- probe_firsttick):
--  * no `TargetPriorities`, so it never finds a target of its own (moho's
--    search passes every candidate over without priority classes,
--    MohoEngine.dll 0x101c5178): a beetle left beside an enemy, its Suicide
--    kept off, never acquires with the ring;
--  * on an attack order the Suicide fires and kills the beetle; the ring,
--    holding the same target, gets no OnFire after that;
--  * Damage 0 target trackers that have priorities (the mobile shield's)
--    acquire and call OnFire at their rate, as in FAF.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        SetAlliance('ARMY_1', 'ARMY_2', 'Enemy')
        SetAlliance('ARMY_2', 'ARMY_1', 'Enemy')
        local function make(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
        local function target(x, z)
            local t = make('uel0201', 'ARMY_2', x, z)
            t:SetFireState(1)
            t:SetMaxHealth(30000) t:SetHealth(nil, 30000)
            return t
        end
        -- A far-off unit keeps ARMY_2 in the game.
        make('ueb3101', 'ARMY_2', 600, 300)
        local counts = {}
        local function hook(tag, u)
            for i = 1, u:GetWeaponCount() do
                local wep = u:GetWeapon(i)
                local label = wep:GetBlueprint().Label or ('weapon' .. i)
                local key = tag .. ' ' .. label
                counts[key] = { got = 0, fire = 0 }
                local c = counts[key]
                local onFire, onGot = wep.OnFire, wep.OnGotTarget
                wep.OnFire = function(self) c.fire = c.fire + 1 return onFire(self) end
                wep.OnGotTarget = function(self) c.got = c.got + 1 return onGot(self) end
            end
        end
        -- Idle beetle, its Suicide held off whatever its script asks.
        target(344, 900)
        local idle = make('xrl0302', 'ARMY_1', 344, 896)
        hook('idle', idle)
        local suicide = idle:GetWeaponByLabel('Suicide')
        suicide:SetEnabled(false)
        suicide.SetEnabled = function() end
        -- Attacking beetle, 3 from its target.
        local t = target(384, 899)
        local atk = make('xrl0302', 'ARMY_1', 384, 896)
        hook('atk', atk)
        -- Kept on (its script turns it on while the beetle is Attacking).
        local live = atk:GetWeaponByLabel('Suicide')
        live:SetEnabled(true)
        live.SetEnabled = function() end
        IssueAttack({ atk }, t)
        -- Mobile shield's target tracker (Damage 0, ALLUNITS priority).
        target(424, 906)
        local shield = make('uel0307', 'ARMY_1', 424, 896)
        hook('shield', shield)
        -- A tank beside its own target: the site sees and engages.
        target(464, 906)
        hook('tank', make('uel0201', 'ARMY_1', 464, 896))
        WaitTicks(50)
        local ring = 'ExplosionRadiusRingDummyWeapon'
        check(counts['idle ' .. ring].got == 0, 'idle ring acquired nothing: got ' .. counts['idle ' .. ring].got)
        check(counts['idle ' .. ring].fire == 0, 'idle ring never fired: ' .. counts['idle ' .. ring].fire)
        check(counts['atk Suicide'].fire == 1, 'attacking beetle fired its Suicide once: ' .. counts['atk Suicide'].fire)
        check(atk.Dead or atk:BeenDestroyed(), 'attacking beetle died')
        check(counts['atk ' .. ring].fire == 0, 'the ring of a beetle its Suicide killed never fired: ' .. counts['atk ' .. ring].fire)
        local tracker
        for k, c in counts do
            if string.find(k, '^shield ') then tracker = c end
        end
        check(tracker and tracker.got == 1, 'shield target tracker acquired')
        check(counts['tank MainGun'].fire >= 2, 'control tank engaged: ' .. counts['tank MainGun'].fire)
        check(tracker and tracker.fire >= 2, 'shield target tracker calls OnFire at its rate: ' .. (tracker and tracker.fire or 0))
    end)
    if not ok then check(false, 'error: ' .. tostring(err)) end
    ScenarioResult.done = true
end)
