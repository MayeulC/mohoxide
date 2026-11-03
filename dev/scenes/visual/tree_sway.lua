-- Visual check for swaying trees: rows of the props whose LOD0 draws
-- with FA's `UndulatingNormalMappedAlpha` (pine, oak, birch, palm, ficus,
-- manzanita). Shoot two ticks a quarter period apart (sin^2 at 0.05 a tick:
-- 63 ticks a sway) at the same camera:
-- cargo run -- --no-ai --script dev/scenes/visual/tree_sway.lua --camera 400,602,30 --screenshot a.png --screenshot-tick 40
-- cargo run -- --no-ai --script dev/scenes/visual/tree_sway.lua --camera 400,602,30 --screenshot b.png --screenshot-tick 56
ForkThread(function()
    WaitTicks(2)
    local rows = {
        { '/env/evergreen/props/trees/pine06_prop.bp', '/env/evergreen/props/trees/pine06_big_prop.bp', '/env/evergreen/props/trees/oak01_s1_prop.bp' },
        { '/env/evergreen/props/trees/brch01_s1_prop.bp', '/env/evergreen/props/trees/dc01_s1_prop.bp', '/env/tropical/props/trees/palm01_s1_prop.bp' },
        { '/env/tropical/props/trees/ficus01_s1_prop.bp', '/env/desert/props/trees/manzanita01_s1_prop.bp', '/env/evergreen/props/trees/pine06_s2_prop.bp' },
    }
    for r, row in rows do
        for c, bp in row do
            local x, z = 392 + c * 4, 594 + r * 4
            CreateProp({ x, GetTerrainHeight(x, z), z }, bp)
        end
    end
end)
