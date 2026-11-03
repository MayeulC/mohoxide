-- Interface half of the table order probe: the construction
-- panel lays out the enhancement tabs in the order `pairs` walks the user
-- layer's `Enhancements.Slots`. Run from the gamemain hook (dev/probes/ui_hooks).
--   ours: cargo run -- --session --player 1 --ticks 100 --ui-lua "$(cat dev/probes/maps/probe_order/ui_probe.lua)"

local function order(name, t)
    local s = ''
    for k in t do s = s .. ' ' .. tostring(k) end
    LOG('PROBE ui ' .. name .. ':' .. s)
end

for _, id in { 'uel0001', 'ual0001', 'url0001', 'xsl0001', 'uel0301', 'ual0301', 'url0301', 'xsl0301' } do
    local bp = __blueprints[id]
    if bp then
        order(id .. ' Slots', bp.Enhancements.Slots)
        order(id .. ' Weapon[1]', bp.Weapon[1])
        order(id .. ' Display', bp.Display)
    end
end
order('lit Back LCH RCH', { Back = 1, LCH = 2, RCH = 3 })
