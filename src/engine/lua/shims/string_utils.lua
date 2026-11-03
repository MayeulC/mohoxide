-- Moho string helpers (Luau: small leaf functions are cheaper than native
-- callbacks; see CLAUDE.md).
local gmatch, gsub, insert = string.gmatch, string.gsub, table.insert
local patternCache = {}

-- Splits on any character in `delimiter` (strtok semantics: empty tokens skipped).
function STR_GetTokens(str, delimiter)
    local pattern = patternCache[delimiter]
    if not pattern then
        pattern = "[^" .. gsub(delimiter, "(%W)", "%%%1") .. "]+"
        patternCache[delimiter] = pattern
    end
    local out = {}
    for token in gmatch(str, pattern) do
        insert(out, token)
    end
    return out
end

-- moho Basename(fullPath, stripExtension): the last component of a path
-- (either slash), without its extension when asked.
function Basename(fullPath, stripExtension)
    local name = (gsub(fullPath, "^.*[/\\]", ""))
    if stripExtension then
        name = (gsub(name, "%.[^.]*$", ""))
    end
    return name
end

-- moho STR_Utf8Len(str): the characters (not bytes) in a UTF-8 string;
-- bytes when it isn't valid UTF-8.
function STR_Utf8Len(str)
    return utf8.len(str) or string.len(str)
end

-- moho STR_Utf8SubString(str, start, count): `count` characters from the
-- `start`th (1-based), clipped to the string.
function STR_Utf8SubString(str, start, count)
    local len = string.len(str)
    local i = utf8.offset(str, start) or (len + 1)
    local j = utf8.offset(str, start + count) or (len + 1)
    return string.sub(str, i, j - 1)
end

-- moho STR_itox(int): lower-case hex, as a 32-bit unsigned value.
function STR_itox(int)
    return string.format("%x", bit32.band(int, 0xFFFFFFFF))
end

-- moho STR_xtoi(str): the leading hex digits as an integer (strtoul: 0 when
-- there are none).
function STR_xtoi(str)
    local digits = string.match(str, "^%s*(%x+)")
    return digits and tonumber(digits, 16) or 0
end
