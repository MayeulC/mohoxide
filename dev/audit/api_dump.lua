-- Engine API dump (docs/guides/audits.md): logs one APIDUMP line per class with
-- every global / moho method the VM has. Sim: --script; interface: --ui-lua.
local function keys(t, seen, acc)
    seen = seen or {}; acc = acc or {}
    if type(t) ~= 'table' or seen[t] then return acc end
    seen[t] = true
    for k, v in next, t do
        if type(k) == 'string' and acc[k] == nil then acc[k] = type(v) end
    end
    local mt = getmetatable(t)
    if type(mt) == 'table' then
        local ix = rawget(mt, '__index')
        if type(ix) == 'table' and ix ~= _G then keys(ix, seen, acc) end
    end
    return acc
end
local layer = rawget(_G, 'APIDUMP_LAYER') or 'SIM'
local function emit(cls, acc)
    local parts = {}
    for k, v in acc do table.insert(parts, k .. ':' .. v) end
    table.sort(parts)
    LOG('APIDUMP ' .. layer .. ' ' .. cls .. ' ' .. table.concat(parts, ','))
end
local g = {}
for k, v in next, _G do if type(k) == 'string' then g[k] = type(v) end end
emit('<global>', g)
if type(moho) == 'table' then
    for cname, ct in next, moho do
        if type(ct) == 'table' then emit('moho.' .. cname, keys(ct)) end
    end
end
for _, lib in {'math', 'string', 'table', 'os', 'io', 'debug', 'coroutine'} do
    local t = rawget(_G, lib)
    if type(t) == 'table' then emit('lib.' .. lib, keys(t)) end
end
local st = rawget(_G, '__STUBS')
local live = {}
if st then
    for key, f in next, st do
        local cur
        local cls, m = string.match(key, '^moho%.([%w_]+)%.([%w_]+)$')
        if cls then cur = rawget(moho[cls], m) else cur = rawget(_G, key) end
        live[key] = (cur == f) and 'STUB' or 'real'
    end
end
emit('<stubs>', live)
LOG('APIDUMP DONE')
