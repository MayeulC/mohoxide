-- Differential probe: `pairs` order of string-keyed tables. FAF
-- lays out the enhancement tabs in the order `pairs` walks a commander's
-- `Enhancements.Slots`, so the tab order is the Lua table's hash order. One
-- "PROBE" line per table: its name, then its keys in iteration order.
--   real game (Windows): dev/probes/run_faf.ps1 probe_order 60
--   ours: cargo run -- --headless --no-ai --ticks 5 --script dev/probes/maps/probe_order/probe.lua

local function order(name, t)
    local s = ''
    for k in t do s = s .. ' ' .. tostring(k) end
    LOG('PROBE ' .. name .. ':' .. s)
end

for _, id in { 'uel0001', 'ual0001', 'url0001', 'xsl0001', 'uel0301', 'ual0301', 'url0301', 'xsl0301' } do
    local bp = __blueprints[id]
    if bp then
        order(id .. ' Slots', bp.Enhancements.Slots)
        order(id .. ' Weapon[1]', bp.Weapon[1])
        order(id .. ' Display', bp.Display)
    end
end

-- Literal tables: insertion order and size, to tell the string hash apart.
order('lit Back LCH RCH', { Back = 1, LCH = 2, RCH = 3 })
order('lit LCH Back RCH', { LCH = 1, Back = 2, RCH = 3 })
order('lit RCH LCH Back', { RCH = 1, LCH = 2, Back = 3 })
local grown = {}
grown.Back = 1 grown.LCH = 2 grown.RCH = 3
order('grown Back LCH RCH', grown)
order('lit a..h', { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 })
order('lit abc', { abc = 1, abd = 2, xyz = 3, Foo = 4, Bar = 5 })
order('lit long', { ['aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1'] = 1, ['aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa2'] = 2, ['b'] = 3, ['c'] = 4 })
-- The same blueprint parsed again, unregistered: does the parse or the
-- registration give the order?
local parsed = {}
local env = { UnitBlueprint = function(bp) parsed = bp end, Sound = function(t) return t end }
setmetatable(env, { __index = _G })
doscript('/units/UEL0001/UEL0001_unit.bp', env)
if parsed.Enhancements then order('parsed uel0001 Slots', parsed.Enhancements.Slots) end
order('lit tables Back LCH RCH', { Back = { name = 'a', x = 1, y = 2 }, LCH = { name = 'b', x = 1, y = 2 }, RCH = { name = 'c', x = 1, y = 2 }, })

-- The interface half (ui_probe.lua) logs as the game interface comes up; give
-- it time before ending the run.
ForkThread(function()
    WaitSeconds(3)
    LOG('PROBE done')
end)
