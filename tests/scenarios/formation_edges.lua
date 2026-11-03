-- Formation edges (tests/scenarios.rs `formation_edges`, which reads the
-- slots from the engine): 24 tanks in AttackFormation sent to
-- FormationEdgesTarget, where part of the layout lands on a cliff or in the
-- sea. Blocked slots must fold in to open ground near the block, and every
-- tank must end on its slot.
ForkThread(function()
    WaitTicks(2)
    local target = rawget(_G, 'FormationEdgesTarget') or {690, 0, 470}
    local units = {}
    for i = 0, 23 do
        local x, z = 660 + math.mod(i, 6) * 3, 420 + math.floor(i / 6) * 3
        local u = CreateUnitHPR('uel0201', 'ARMY_1', x, GetTerrainHeight(x, z), z, 0, 0, 0)
        u:SetFireState(1)
        table.insert(units, u)
    end
    IssueFormMove(units, target, 'AttackFormation', 0)
end)
