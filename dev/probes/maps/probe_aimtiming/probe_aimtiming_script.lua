-- Map script for the aim timing probe (weapon aim phase 4) in the real game: sandbox victory
-- (set before the session picks its victory condition) so the probe can clear
-- the armies' start units, then the shared probe body (also run by our
-- engine via --script).
local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')

function OnPopulate()
    ScenarioInfo.Options.Victory = 'sandbox'
    ScenarioUtils.InitializeArmies()
end

function OnStart(self)
    import('/maps/probe_aimtiming/probe.lua')
end
