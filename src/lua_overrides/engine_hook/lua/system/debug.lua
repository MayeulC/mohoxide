-- Engine hook: FAF's `debug.allocatedrsize` walks a structure in Lua, a
-- call into the engine per value; the nav mesh report after generation
-- (`/lua/sim/NavGenerator.lua`) spends ~0.6 s in it (release). The engine's
-- native walk (`src/engine/api/debug.rs`) gives the same total.
if debug.__allocatedrsize then
    debug.allocatedrsize = debug.__allocatedrsize
end
