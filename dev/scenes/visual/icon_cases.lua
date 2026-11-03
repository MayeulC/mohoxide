-- Strategic icon cases on the land patch of scmp_009: a row of idle
-- aircraft, tanks and a structure per army, to look at the icons over the
-- meshes from the icon zoom (130 plus its fade: 150 and out). Run with
-- --no-ai.
--
-- Screenshot: cargo run -- --script dev/scenes/visual/icon_cases.lua --no-ai --screenshot x.png --screenshot-tick 30 --camera 400,600,150
-- Each unit logs its entity id (ICONCASE lines in game.log) for --select;
-- a start selection is applied at 6 s of interface time (--screenshot-tick 80).
local function spawn(bp, army, x, z, alt)
    return CreateUnitHPR(bp, army, x, GetSurfaceHeight(x, z) + (alt or 0), z, 0, 0, 0)
end

local rows = {
    { 'uea0102', 'uea0203', 'uel0201', 'ueb2301' }, -- UEF: fighter, gunship, tank, point defence
    { 'uaa0102', 'uaa0203', 'ual0201', 'uab2301' }, -- Aeon
    { 'ura0102', 'ura0203', 'url0107', 'urb2301' }, -- Cybran
    { 'xsa0102', 'xsa0203', 'xsl0201', 'xsb2301' }, -- Seraphim
}
for army = 1, 4 do
    local row = rows[army]
    for i, bp in ipairs(row) do
        local u = spawn(bp, army, 340 + i * 20, 540 + army * 25, i <= 2 and 10 or 0)
        if u then
            u:SetCanTakeDamage(false)
            LOG(string.format("ICONCASE %s army %d id %s", bp, army, tostring(u:GetEntityId())))
        end
    end
end
