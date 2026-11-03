-- Visual check for FA's clutter techniques (`ClutterPS`, alpha-tested at
-- 0x70): the ground clutter props and kelp in a row, each three times.
-- cargo run -- --no-ai --script dev/scenes/visual/clutter_cases.lua --camera 400,600,12 --screenshot c.png --screenshot-tick 20
ForkThread(function()
    WaitTicks(2)
    local props = {
        '/env/common/props/clutter01_prop.bp',
        '/env/common/props/clutter02_prop.bp',
        '/env/common/props/clutter03_prop.bp',
        '/env/common/props/kelp_01_prop.bp',
    }
    for c, bp in props do
        for r = 0, 2 do
            local x, z = 393 + c * 3, 597 + r * 3
            CreateProp({ x, GetTerrainHeight(x, z), z }, bp)
        end
    end
end)
