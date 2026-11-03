-- Blueprint field dump (dev/audit/bp_audit.py): logs one BPFIELD line per
-- field path of the loaded blueprints (after FAF's blueprint loading has
-- added and changed fields), with how many blueprints set it and a sample.
-- Array elements are `[]` (`Weapon[].RateOfFire`). Run:
--   cargo run -- --headless --ticks 1 --no-ai --script dev/audit/bp_dump.lua
local fields = {}

local function kind(bp)
    local id = string.lower(tostring(rawget(bp, 'BlueprintId') or ''))
    local src = string.lower(rawget(bp, 'Source') or '')
    if string.find(id, '_mesh', 1, true) then return 'mesh' end
    for _, k in ipairs({ 'emit', 'beam', 'trail', 'proj', 'prop', 'unit' }) do
        if string.find(src, '_' .. k .. '.bp', 1, true) then
            return ({ emit = 'emitter', proj = 'projectile' })[k] or k
        end
    end
    return 'other'
end

local function sample(v)
    local t = type(v)
    if t == 'string' then return string.format('%q', string.sub(v, 1, 40)) end
    if t == 'number' or t == 'boolean' then return tostring(v) end
    return t
end

-- Records each path once per blueprint (`seen`).
local function walk(k, t, path, seen, depth)
    if depth > 8 then return end
    for key, v in next, t do
        local p
        if type(key) == 'number' then
            p = path .. '[]'
        elseif type(key) == 'string' then
            p = path == '' and key or (path .. '.' .. key)
        end
        if p then
            local id = k .. ' ' .. p
            if not seen[id] then
                seen[id] = true
                local f = fields[id]
                if not f then
                    f = { count = 0, sample = sample(v) }
                    fields[id] = f
                end
                f.count = f.count + 1
            end
            if type(v) == 'table' then walk(k, v, p, seen, depth + 1) end
        end
    end
end

local done = {}
local total = {}
for _, bp in next, __blueprints do
    if type(bp) == 'table' and not done[bp] then
        done[bp] = true
        local k = kind(bp)
        total[k] = (total[k] or 0) + 1
        walk(k, bp, '', {}, 0)
    end
end

local lines = {}
for id, f in next, fields do
    table.insert(lines, id .. ' ' .. f.count .. ' ' .. f.sample)
end
table.sort(lines)
for k, n in next, total do LOG('BPTOTAL ' .. k .. ' ' .. n) end
for _, l in ipairs(lines) do LOG('BPFIELD ' .. l) end
LOG('BPDONE ' .. table.getn(lines))
