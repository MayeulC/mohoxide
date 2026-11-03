-- dev/audit/scenario.lua: the weapon audit's per-weapon scenario, read from
-- the blueprint alone (dev/audit/weapon_audit.lua reads
-- WEAPON_AUDIT_SCENARIO from here). Pure: no sim calls, no state, so the
-- same blueprint always gives the same scenario.
-- Canonical route: cat-join dev/audit/targets.lua, this file and
-- dev/audit/weapon_audit.lua and run the three as one --script.
--
-- scenario_of(bp, wbp, wi, catalog) -> {
--     kinds = the target kinds the weapon may engage, in preference order
--             (catalog keys: building, land, ship, sub, fighter),
--     unsupported = reason sentence | nil, reasonKey = short token | nil }
-- Target kind: the catalog targets the weapon's TargetRestrictOnlyAllow /
-- TargetRestrictDisallow forbid are dropped (parsed as the engine does,
-- src/engine/api/categories.rs parse_restriction: ',' unites terms, ' '
-- intersects the names within a term, over the catalog bp's categories).
-- Order: building, land (mobile tank), ship (surface), sub, fighter; AA
-- first (fighter leading) when OnlyAllow names AIR or WeaponCategory is
-- "Anti Air". The weapon's layer caps then pick the first kind on a layer
-- it can fire at (pick_kind): the live caps stay authoritative for layers.
-- TargetType RULEWTT_Projectile: the weapon engages projectiles, not units.
-- A tactical anti-missile (OnlyAllow names neither TORPEDO nor STRATEGIC)
-- gets intercept = 'tactical', source = 'tml' (the catalog's tactical
-- launcher fires at it); a strategic one (OnlyAllow STRATEGIC MISSILE)
-- intercept = 'strategic', source = 'nuke'; an anti-torpedo one (OnlyAllow
-- TORPEDO) intercept = 'torpedo', source = 'torpedo'.
-- attacker_layer(bp, wbp): the layer the attacker must stand on to fire
-- (below).

-- The layer caps a kind's target stands on (a sub is spawned surfaced, so
-- any wet layer reaches it: the old naval kind's rule).
local KIND_LAYERS = {
    building = { LAND = true },
    land = { LAND = true },
    ship = { WATER = true },
    sub = { WATER = true, SUB = true, SEABED = true },
    fighter = { AIR = true },
}
-- naval: the old kind name, an alias of ship (override rows may name it)
KIND_LAYERS.naval = KIND_LAYERS.ship
-- Where a kind's target is spawned: any = airborne (no medium constraint).
local KIND_ENV = { building = 'dry', land = 'dry', ship = 'wet', naval = 'wet', sub = 'wet', fighter = 'any' }

local GROUND_FIRST = { 'building', 'land', 'ship', 'sub', 'fighter' }
local AA_FIRST = { 'fighter', 'building', 'land', 'ship', 'sub' }

-- Does a category hash match a restriction string? nil when the string
-- has no terms (no restriction).
local function restriction_matches(hash, text)
    if type(text) ~= 'string' then return nil end
    local any = false
    for term in string.gmatch(text, '[^,]+') do
        local all, named = true, false
        for name in string.gmatch(term, '%S+') do
            named = true
            if hash[name] == nil then
                all = false
                break
            end
        end
        if named then
            any = true
            if all then return true end
        end
    end
    if not any then return nil end
    return false
end

-- May the weapon engage a unit of this blueprint?
local function allows(wbp, tbp)
    local hash = (tbp and tbp.CategoriesHash) or {}
    if restriction_matches(hash, wbp.TargetRestrictOnlyAllow) == false then return false end
    if restriction_matches(hash, wbp.TargetRestrictDisallow) == true then return false end
    return true
end

local function names(text, name)
    if type(text) ~= 'string' then return false end
    for term in string.gmatch(text, '[^,]+') do
        for n in string.gmatch(term, '%S+') do
            if n == name then return true end
        end
    end
    return false
end

-- The catalog row key (an id prefix) of each faction category.
local FACTION_ROW = { UEF = 'ue', AEON = 'ua', CYBRAN = 'ur', SERAPHIM = 'xs' }

-- The catalog bp id of a kind for an attacker id: the row of its faction
-- category (xa / xr / xe / d... ids carry no faction prefix of their own),
-- else its id prefix, else the UEF row.
local function target_of(catalog, kind, attackerId)
    local row = catalog[kind]
    if not row then return nil end
    local bp = attackerId and __blueprints[attackerId]
    local h = bp and bp.CategoriesHash or {}
    for _, cat in { 'UEF', 'AEON', 'CYBRAN', 'SERAPHIM' } do
        if h[cat] ~= nil and row[FACTION_ROW[cat]] then return row[FACTION_ROW[cat]] end
    end
    return row[string.sub(attackerId or '', 1, 2)] or row.ue
end

local function scenario_of(bp, wbp, wi, catalog)
    local out = { kinds = {} }
    if wbp.TargetType == 'RULEWTT_Projectile' then
        local only = wbp.TargetRestrictOnlyAllow
        -- tactical anti-missile (OnlyAllow TACTICAL MISSILE, or
        -- TACTICAL,MISSILE): stops the faction tactical launcher's missile
        if not names(only, 'TORPEDO') and not names(only, 'STRATEGIC') then
            out.intercept = 'tactical'
            out.source = 'tml'
            return out
        end
        -- strategic missile defence (OnlyAllow STRATEGIC MISSILE): stops the
        -- faction nuke launcher's missile
        if names(only, 'STRATEGIC') and not names(only, 'TORPEDO') then
            out.intercept = 'strategic'
            out.source = 'nuke'
            return out
        end
        -- anti-torpedo (OnlyAllow TORPEDO): stops the faction torpedo
        -- launcher's torpedoes
        if names(only, 'TORPEDO') then
            out.intercept = 'torpedo'
            out.source = 'torpedo'
            return out
        end
        local sub = names(only, 'TORPEDO') and 'torp' or 'missile'
        out.unsupported = 'anti-projectile (' .. (sub == 'torp' and 'anti-torpedo' or 'anti-missile')
            .. '): engages projectiles, not units'
        out.reasonKey = 'anti-proj:' .. sub
        return out
    end
    local id = string.lower(bp.BlueprintId or '')
    local order = (names(wbp.TargetRestrictOnlyAllow, 'AIR') or wbp.WeaponCategory == 'Anti Air')
        and AA_FIRST or GROUND_FIRST
    for _, kind in order do
        local tid = target_of(catalog, kind, id)
        local tbp = tid and __blueprints[tid]
        if tbp and allows(wbp, tbp) then table.insert(out.kinds, kind) end
    end
    if out.kinds[1] == nil then
        out.unsupported = 'restrictions: no catalog target passes TargetRestrictOnlyAllow / Disallow'
        out.reasonKey = 'restrict'
    end
    return out
end

-- The first of the scenario's kinds on a layer in the caps string
-- ('Air|Land|...'): kind, spawn env; nil when none is.
local function pick_kind(kinds, caps)
    local set = {}
    for s in string.gmatch(caps or '', '[^|]+') do set[string.upper(s)] = true end
    for _, kind in kinds do
        for layer in KIND_LAYERS[kind] or {} do
            if set[layer] then return kind, KIND_ENV[kind] end
        end
    end
    return nil, nil
end

-- Attacker layer: where the attacker must stand for the weapon to fire.
-- The layers its motion type can occupy (structures, RULEUMT_None: the
-- BuildOnLayerCaps marked true), in the preference order used when the
-- natural layer is not a firing layer.
local LAYER_ORDER = { 'Land', 'Water', 'Seabed', 'Sub' }
local OCCUPY = {
    RULEUMT_Land = { Land = true },
    RULEUMT_Biped = { Land = true },
    RULEUMT_Amphibious = { Land = true, Seabed = true },
    RULEUMT_Hover = { Land = true, Water = true },
    RULEUMT_AmphibiousFloating = { Land = true, Water = true },
    RULEUMT_Water = { Water = true },
    RULEUMT_SurfacingSub = { Water = true, Sub = true },
    RULEUMT_Sub = { Sub = true },
}

local function is_air_bp(bp)
    local h = bp.CategoriesHash or {}
    return h.AIR ~= nil or (bp.Physics and bp.Physics.MotionType or '') == 'RULEUMT_Air'
end

-- The layers a unit can stand on, and the one it takes when spawned on its
-- natural ground (wet for naval units, a structure's first build layer).
local function occupiable(bp)
    local p = bp.Physics or {}
    local mt = p.MotionType or ''
    if is_air_bp(bp) then return { Air = true }, 'Air' end
    local h = bp.CategoriesHash or {}
    local set = OCCUPY[mt]
    if set == nil then
        -- structures (RULEUMT_None) and anything unknown: BuildOnLayerCaps
        set = {}
        -- the bp table ({ LAYER_Water = true, ... }) or the loaded form, a
        -- mask string ('Water', 'Land|Seabed')
        local caps = p.BuildOnLayerCaps
        if type(caps) == 'table' then
            for _, l in LAYER_ORDER do
                if caps['LAYER_' .. l] then set[l] = true end
            end
        elseif type(caps) == 'string' then
            for l in string.gmatch(caps, '[^|]+') do
                set[(string.gsub(l, '^LAYER_', ''))] = true
            end
        end
        if next(set) == nil then set.Land = true end
        for _, l in LAYER_ORDER do
            if set[l] then return set, l end
        end
    end
    local natural = 'Land'
    if mt == 'RULEUMT_Water' then
        natural = 'Water'
    elseif mt == 'RULEUMT_Sub' then
        natural = 'Sub'
    elseif mt == 'RULEUMT_SurfacingSub' then
        -- spawned submerged unless experimental (moho's spawn layer,
        -- src/engine/api/unit.rs); IssueDive changes depth
        natural = h.EXPERIMENTAL ~= nil and 'Water' or 'Sub'
    elseif (mt == 'RULEUMT_Hover' or mt == 'RULEUMT_AmphibiousFloating') and h.NAVAL ~= nil then
        natural = 'Water'
    end
    return set, natural
end

-- attacker_layer(bp, wbp) -> { layer = planned firing layer, spawn = the
-- layer it takes on spawn, medium = 'dry' | 'wet' (where it spawns),
-- dive = true when a SurfacingSub must change depth (IssueDive) to reach
-- layer }. Firing layers: the keys of FireTargetLayerCapsTable with a
-- non-empty value that the unit can occupy. The natural layer wins when it
-- is one; else the first of Land, Water, Seabed, Sub. None (no caps table,
-- or no reachable key): the natural layer, and the live caps give the
-- verdict (no-caps).
local function attacker_layer(bp, wbp)
    local set, natural = occupiable(bp)
    local fire = {}
    local caps = wbp.FireTargetLayerCapsTable
    if type(caps) == 'table' then
        for l, v in caps do
            if type(v) == 'string' and string.find(v, '%a') and set[l] then fire[l] = true end
        end
    end
    local layer = natural
    if not fire[natural] then
        for _, l in LAYER_ORDER do
            if fire[l] then
                layer = l
                break
            end
        end
    end
    local mt = bp.Physics and bp.Physics.MotionType or ''
    local spawn, dive = layer, false
    if mt == 'RULEUMT_SurfacingSub' and layer ~= natural then
        spawn, dive = natural, true
    end
    local medium = (spawn == 'Land' or spawn == 'Air') and 'dry' or 'wet'
    return { layer = layer, spawn = spawn, medium = medium, dive = dive }
end

rawset(_G, 'WEAPON_AUDIT_SCENARIO', {
    attacker_layer = attacker_layer,
    scenario_of = scenario_of,
    pick_kind = pick_kind,
    allows = allows,
    target_of = target_of,
    kind_env = KIND_ENV,
})
