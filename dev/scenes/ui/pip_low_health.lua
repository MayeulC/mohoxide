-- Wears ARMY_1's ACU down below 10% health, for the PiP example mod
-- (dev/vault/mods/pip_acu_low_health; docs/architecture/pip.md):
-- cargo run -- --session --no-audio --vault dev/vault --mod 5e3c1f0a-pip-acu-low-health --script dev/scenes/ui/pip_low_health.lua --screenshot pip_low.png --screenshot-tick 125
ForkThread(function()
    -- (The interface is up from about tick 60.)
    WaitTicks(70)
    local acu = ArmyBrains[1]:GetListOfUnits(categories.COMMAND, false)[1]
    if not acu then
        WARN('dev/pip_low_health: ARMY_1 has no ACU')
        return
    end
    -- 5% every 2 ticks: below 10% at about tick 106.
    for i = 1, 19 do
        WaitTicks(2)
        if acu.Dead then return end
        Damage(nil, acu:GetPosition(), acu, acu:GetMaxHealth() * 0.05, 'Normal')
        LOG('dev/pip_low_health: tick ', GetGameTick(), ' health ', acu:GetHealth() / acu:GetMaxHealth(), ' asked ', tostring(acu.PiPLowHealth))
    end
end)
