-- Script decal LOD cutoffs on the land patch of scmp_009: three rows of
-- scorches (CreateDecal, permanent), one per lodParam. Zoom out from
--   cargo run --release -- --script dev/scenes/visual/decal_cases.lua --no-ai --camera 380,580,30
-- and they fade out over the last quarter of their cutoff (view depth):
--   z 560  lod 60    gone first
--   z 580  lod 0     moho's default: 7x7 albedo -> diagonal x 15 ~ 148
--   z 600  lod 4000  stays (SimObjectives' DecalLOD)
local rows = { { z = 560, lod = 60 }, { z = 580, lod = 0 }, { z = 600, lod = 4000 } }
for _, row in rows do
    for i = 0, 4 do
        local x = 360 + i * 10
        CreateDecal(Vector(x, GetSurfaceHeight(x, row.z), row.z), i, 'Scorch_generic_002_albedo', '', 'Albedo', 7, 7, row.lod, 0, -1)
    end
end
