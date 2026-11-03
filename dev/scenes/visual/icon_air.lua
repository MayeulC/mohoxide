-- Aircraft under their strategic icons: a T1 fighter, T2 gunship, T3
-- fighter and T3 bomber per army, hovering in place, to look at what the
-- mesh's lowest LOD draws under the icon between the icon zoom (130) and the
-- mesh's last cutoff (215). Run with --no-ai.
--
--   cargo run -- --script dev/scenes/visual/icon_air.lua --no-ai --screenshot x.png --screenshot-tick 30 --camera 400,600,140
local function spawn(bp, army, x, z)
    local u = CreateUnitHPR(bp, army, x, GetSurfaceHeight(x, z) + 12, z, 0, 0, 0)
    if u then
        u:SetCanTakeDamage(false)
    end
    return u
end

local rows = {
    { 'uea0102', 'uea0203', 'uea0303', 'uea0304' }, -- UEF
    { 'uaa0102', 'uaa0203', 'uaa0303', 'uaa0304' }, -- Aeon
    { 'ura0102', 'ura0203', 'ura0303', 'ura0304' }, -- Cybran
    { 'xsa0102', 'xsa0203', 'xsa0303', 'xsa0304' }, -- Seraphim
}
for army = 1, 4 do
    for i, bp in ipairs(rows[army]) do
        spawn(bp, army, 355 + i * 20, 545 + army * 25)
    end
end
