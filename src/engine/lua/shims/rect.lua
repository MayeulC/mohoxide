-- `Rect(x0, y0, x1, y1)`: a plain Rectangle table. A tiny leaf function, so a
-- Luau shim is cheaper than a native callback (see CLAUDE.md).
function Rect(x0, y0, x1, y1)
    return { x0 = x0, y0 = y0, x1 = x1, y1 = y1 }
end
