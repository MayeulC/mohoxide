-- Scenario test (tests/scenarios.rs): hover units lean into their change of
-- speed as the real game's do. A Seraphim T1 engineer (xsl0105,
-- BankingSlope 0.5, wobble 0.015) sent 8 ahead tilts at most ~4 degrees
-- setting off and stopping in the real game (dev/probes/maps/probe_wobble:
-- 3.8); taking moho's per-tick change of velocity for an acceleration
-- leaned it 21 and flipped it on every speed change. A Seraphim T1 AA
-- hover (xsl0205, BankingSlope 0.5, no wobble) levels out after it stops
-- instead of keeping the lean it braked with (8 degrees).
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function tilt(u)
    local q = u:GetOrientation()
    local up_y = 1 - 2 * (q[1] * q[1] + q[3] * q[3])
    return math.deg(math.acos(math.min(1, math.max(-1, up_y))))
end
ForkThread(function()
    WaitTicks(2)
    -- Seton's flat area A, facing east.
    local E = math.pi / 2
    local eng = CreateUnitHPR('xsl0105', 'ARMY_1', 606, GetTerrainHeight(606, 110), 110, 0, E, 0)
    local aa = CreateUnitHPR('xsl0205', 'ARMY_1', 618, GetTerrainHeight(618, 110), 110, 0, E, 0)
    eng:SetFireState(1)
    aa:SetFireState(1)
    WaitTicks(10)
    IssueMove({ eng }, { 606, 0, 118 })
    IssueMove({ aa }, { 618, 0, 118 })
    local most = { eng = 0, aa = 0 }
    for _ = 1, 70 do
        most.eng = math.max(most.eng, tilt(eng))
        most.aa = math.max(most.aa, tilt(aa))
        WaitTicks(1)
    end
    local p = eng:GetPosition()
    check(math.abs(p[3] - 118) < 1, string.format('the engineer got there (z %.2f)', p[3]))
    check(most.eng < 6, string.format('the engineer leans at most ~4 degrees setting off and stopping (real 3.8), got %.2f', most.eng))
    check(most.aa < 6, string.format('the AA hover leans at most ~4 degrees, got %.2f', most.aa))
    check(tilt(aa) < 0.3, string.format('the AA hover is level again at rest, got %.2f degrees', tilt(aa)))
    table.insert(ScenarioResult.notes, string.format('max tilt engineer %.2f, AA %.2f; AA at rest %.3f', most.eng, most.aa, tilt(aa)))
    ScenarioResult.done = true
end)
