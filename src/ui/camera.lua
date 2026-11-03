-- moho's CameraImpl for the interface (`GetCamera('WorldCamera')`,
-- `GetCamera('WorldCamera2')` in split view): reads the renderer's camera of
-- its name (`__engine_cameras`, set every frame by
-- `UserVm::set_camera_states`) and queues moves for it
-- (`__engine_camera_requests`, taken by `UserVm::take_camera_requests`). A
-- camera the renderer has none of (the minimap's) reads the main one's.
local methods = moho.CameraImpl_methods

local default = { focus = { 0, 0, 0 }, zoom = 100, min = 8, max = 1000, pitch = 0, heading = math.pi }
local function state(self)
    local all = rawget(_G, '__engine_cameras')
    if not all then return default end
    return all[rawget(self, '_c_name') or ''] or all.WorldCamera or default
end
local function request(self, r)
    local q = rawget(_G, '__engine_camera_requests')
    if not q then q = {} rawset(_G, '__engine_camera_requests', q) end
    r.camera = rawget(self, '_c_name')
    table.insert(q, r)
end

methods.GetFocusPosition = function(self) local f = state(self).focus return Vector(f[1], f[2], f[3]) end
methods.GetZoom = function(self) return state(self).zoom end
-- The latest zoom asked for this camera and not yet applied, else the
-- current one.
methods.GetTargetZoom = function(self)
    local q = rawget(_G, '__engine_camera_requests')
    local name = rawget(self, '_c_name')
    for i = table.getn(q or {}), 1, -1 do
        if q[i].zoom and q[i].camera == name then return q[i].zoom end
    end
    return state(self).zoom
end
-- The renderer's farthest zoom (its `SetMaxZoomMult` already applied).
methods.GetMaxZoom = function(self) return state(self).max end
methods.GetMinZoom = function(self) return state(self).min end
methods.SetZoom = function(self, zoom, seconds) request(self, { zoom = zoom, seconds = seconds or 0 }) end
methods.SetTargetZoom = function(self, zoom) request(self, { zoom = zoom, seconds = 0 }) end
-- For the world views' cameras (the minimap's sets 1.0 for its own, which
-- the renderer draws from the map image, not a camera).
methods.SetMaxZoomMult = function(self, mult)
    if rawget(self, '_c_name') ~= 'MiniMap' then request(self, { max_zoom_mult = mult }) end
end
-- Heading and pitch in radians, as moho's (0x7b44f0: the camera's +0x34c
-- and +0x344), which `RestoreSettings` and `MoveTo` take back.
methods.SaveSettings = function(self)
    local s = state(self)
    return { Zoom = s.zoom, Pitch = s.pitch, Heading = s.heading, Focus = Vector(s.focus[1], s.focus[2], s.focus[3]) }
end
-- Moho's (0x7b46e0) is `SnapTo(Focus, { Heading, Pitch }, Zoom)`.
methods.RestoreSettings = function(self, settings)
    if settings then request(self, { focus = settings.Focus, zoom = settings.Zoom, heading = settings.Heading, pitch = settings.Pitch, seconds = 0 }) end
end
-- `hpr`: heading (pi faces north), pitch (down from level), roll (unused),
-- radians. The camera turns to it (over `seconds`) and holds it until
-- `RevertRotation` (moho's vtable +0x38, 0x7b02d0).
local function move(self, position, hpr, zoom, seconds)
    local heading, pitch
    if type(hpr) == 'table' then heading, pitch = tonumber(hpr[1]), tonumber(hpr[2]) end
    request(self, { focus = position, zoom = zoom, heading = heading, pitch = pitch, seconds = seconds or 0 })
end
methods.MoveTo = function(self, position, hpr, zoom, seconds) move(self, position, hpr, zoom, seconds) end
methods.SnapTo = function(self, position, hpr, zoom) move(self, position, hpr, zoom, 0) end
methods.MoveToRegion = function(self, region, seconds)
    if not region then return end
    local cx, cz = (region.x0 + region.x1) / 2, (region.y0 + region.y1) / 2
    local size = math.max(region.x1 - region.x0, region.y1 - region.y0)
    request(self, { focus = { cx, GetTerrainHeight and GetTerrainHeight(cx, cz) or 0, cz }, zoom = size, seconds = seconds or 0 })
end
-- Follows the units (user units, or entity ids as the sim's camera requests
-- send) at their middle, from `zoom`, until the camera is moved by hand or
-- they're all gone; `seconds` to get there.
methods.TrackEntities = function(self, units, zoom, seconds)
    local ids = {}
    for _, u in units or {} do
        local id = tonumber(type(u) == 'table' and u.GetEntityId and u:GetEntityId() or u)
        if id then table.insert(ids, id) end
    end
    if table.getn(ids) > 0 then request(self, { track = ids, zoom = zoom, seconds = seconds or 0 }) end
end
methods.Reset = function(self) request(self, { reset = true }) end
-- A camera turned by Space and the mouse (or Insert / Delete) eases back to
-- the usual view; `HoldRotation` keeps its heading and pitch as it zooms.
methods.RevertRotation = function(self) request(self, { revert_rotation = true }) end
methods.HoldRotation = function(self) request(self, { hold_rotation = true }) end
methods.EnableEaseInOut = function(self) end
methods.SetAccMode = function(self, mode) end
methods.Spin = function(self, headingRate, zoomRate) end

-- `UIZoomTo(units, seconds)`: frames the units.
function UIZoomTo(units, seconds)
    local x0, z0, x1, z1
    for _, u in units or {} do
        if u and not u:IsDead() then
            local p = u:GetPosition()
            x0, z0 = math.min(x0 or p[1], p[1]), math.min(z0 or p[3], p[3])
            x1, z1 = math.max(x1 or p[1], p[1]), math.max(z1 or p[3], p[3])
        end
    end
    if not x0 then return end
    local cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
    local size = math.max(x1 - x0, z1 - z0, 25)
    request({}, { focus = { cx, 0, cz }, zoom = size * 1.5, seconds = seconds or 0 })
end
