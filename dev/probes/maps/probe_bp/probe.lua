-- Differential probe: blueprint tables as Lua sees them after registration.
-- The real engine merges its parsed blueprint back into the
-- Lua table, so enums and bitfields come back as text or numbers and fields
-- the blueprint left out may come back with their defaults. One "PROBE" line
-- per leaf: blueprint, path, Lua type, value. Engine APIs only (Lua 5.0 safe).
--   real game (Windows): dev/probes/run_faf.ps1 probe_bp 60
--   ours: cargo run -- --headless --no-ai --ticks 5 --script dev/probes/maps/probe_bp/probe.lua

local function sorted_keys(t)
    local keys = {}
    for k in t do table.insert(keys, k) end
    table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
    return keys
end

local function dump(id, path, v, depth)
    local kind = type(v)
    if kind == 'table' then
        if depth > 6 then
            LOG('PROBE ' .. id .. ' ' .. path .. ' deep')
            return
        end
        local keys = sorted_keys(v)
        if table.getn(keys) == 0 then
            LOG('PROBE ' .. id .. ' ' .. path .. ' table {}')
        end
        for _, k in keys do
            dump(id, path .. '.' .. tostring(k), v[k], depth + 1)
        end
    elseif kind == 'string' or kind == 'number' or kind == 'boolean' then
        LOG('PROBE ' .. id .. ' ' .. path .. ' ' .. kind .. ' ' .. tostring(v))
    else
        LOG('PROBE ' .. id .. ' ' .. path .. ' ' .. kind)
    end
end

-- Units across layers and kinds: land, structure, naval, air, a factory,
-- an engineer, a naval factory, amphibious units, structures built on
-- several layers (extractors, torpedo launchers, sonar).
local units = { 'uel0101', 'ueb1101', 'ues0103', 'uea0303', 'ueb0101', 'uel0105', 'ueb0103', 'url0203', 'xsl0103', 'uab1103', 'ueb2109', 'ueb3102', 'xsb2109' }
for _, id in units do
    local bp = __blueprints[id]
    if bp then dump(id, 'bp', bp, 0) else LOG('PROBE ' .. id .. ' missing') end
end

-- One of each other blueprint kind, by the first matching key.
local others = { projectile = '/projectiles/tdfgauss01/', prop = '/props/', mesh = '_mesh', emitter = '_emit.bp', beam = '_beam.bp', trail = '_trail.bp' }
local keys = {}
for k in __blueprints do
    if type(k) == 'string' then table.insert(keys, k) end
end
table.sort(keys)
for _, kind in sorted_keys(others) do
    local found
    for _, k in keys do
        if string.find(k, others[kind], 1, true) then
            found = k
            break
        end
    end
    if found then dump(kind, 'bp', __blueprints[found], 0) else LOG('PROBE ' .. kind .. ' missing') end
end


ForkThread(function()
    WaitTicks(2)
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
