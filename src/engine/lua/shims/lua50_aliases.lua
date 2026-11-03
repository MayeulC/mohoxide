-- Lua 5.0 standard library names FAF still uses, mapped onto Luau equivalents.
math.mod = math.mod or math.fmod
math.pow = math.pow or function(a, b) return a ^ b end
string.gfind = string.gfind or string.gmatch
-- Lua 5.0 explicit table size; Luau tracks sizes itself, so it's a no-op.
table.setn = table.setn or function(t, n) end

-- Lua 5.0 generic-for over a table uses `next` (see lua_translation/forin.rs).
local next, type = next, type
local empty = {}
function __forin(t)
    if type(t) == "table" then
        return next, t
    end
    -- Iterating nil is an empty loop in moho's Lua (FAF relies on it, e.g.
    -- `for _, v in bp.Weapon do` for weaponless engineers in Unit.lua).
    if t == nil then
        return next, empty
    end
    return t
end

-- Lua 5.0 `unpack` takes its length from `table.getn`, which honours an `n`
-- field: `unpack(arg)` passes every vararg, nils included. Luau's stops at
-- `#t`, which a nil hole can cut short (FAF's UIFactory passes `nil, nil,
-- ..., prefID` through `unpack(arg)`; the window lost its pref ID).
local tunpack, rawget, ttype = table.unpack or unpack, rawget, type
function unpack(t, i, j)
    if j == nil and ttype(t) == "table" then
        local n = rawget(t, "n")
        if ttype(n) == "number" then
            return tunpack(t, i or 1, n)
        end
    end
    return tunpack(t, i or 1, j)
end
