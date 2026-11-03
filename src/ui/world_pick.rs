//! Screen to world for the interface: `UnProject(view, {x, y})` and
//! `GetMouseWorldPos()` (FAF's radial dragger, ping and marker tools), and
//! world to screen for a world view (`Project`). The renderer hands over
//! each world view's camera each frame (`UserVm::set_pick_views`); a point
//! is where the camera ray through the screen position meets the terrain,
//! found as the renderer's own picking does (march, then bisect). Interface
//! coordinates are physical pixels / `ui_scale`.

use crate::lua_api::shapes;
use crate::ui::shapes::{Keyed, Loose};
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};
use mlua::prelude::*;

use crate::engine::api::moho;
use crate::engine::snapshot::TerrainView;
use crate::lua_api::{Obj, lua_fn, lua_method};

/// A world view's camera this frame, as the interface sees it.
#[derive(Clone)]
pub struct PickView {
    /// The camera's name (`WorldCamera`, `WorldCamera2`, ...).
    pub camera: String,
    /// The view's top left in the window (physical pixels).
    pub origin: [f32; 2],
    pub view_proj: Mat4,
    pub inv_view_proj: Mat4,
    /// The view's size (physical pixels).
    pub screen: [f32; 2],
    pub ui_scale: f32,
    /// The camera's look-at height (rays that miss the ground meet it there)
    /// and distance (sets the march step).
    pub target_y: f32,
    pub distance: f32,
    pub terrain: Option<Arc<TerrainView>>,
}

impl PickView {
    fn height_at(&self, x: f32, z: f32) -> f32 {
        let Some(t) = &self.terrain else { return self.target_y };
        let xi = (x.max(0.0) as u32).min(t.width);
        let zi = (z.max(0.0) as u32).min(t.height);
        t.heights.get((zi * (t.width + 1) + xi) as usize).copied().unwrap_or(self.target_y)
    }

    /// Interface coordinates of world point `p` from the view's top left
    /// (moho's `Project` is in the view's own space; behind the camera: far
    /// off screen the way it points).
    pub fn project(&self, p: Vec3) -> (f32, f32) {
        let clip = self.view_proj * p.extend(1.0);
        let w = if clip.w.abs() < 1e-6 { 1e-6 } else { clip.w };
        let (nx, ny) = (clip.x / w, clip.y / w);
        let (nx, ny) = if clip.w < 0.0 { (-nx * 1e4, -ny * 1e4) } else { (nx, ny) };
        let px = (nx + 1.0) * 0.5 * self.screen[0];
        let py = (1.0 - ny) * 0.5 * self.screen[1];
        (px / self.ui_scale, py / self.ui_scale)
    }

    /// Whether interface point (x, y) is in the view.
    fn contains(&self, x: f32, y: f32) -> bool {
        let (px, py) = (x * self.ui_scale - self.origin[0], y * self.ui_scale - self.origin[1]);
        px >= 0.0 && py >= 0.0 && px < self.screen[0] && py < self.screen[1]
    }

    /// The ground under interface point (x, y) (from the window's top
    /// left), if any.
    pub fn ground(&self, x: f32, y: f32) -> Option<Vec3> {
        let (px, py) = (x * self.ui_scale - self.origin[0], y * self.ui_scale - self.origin[1]);
        if self.screen[0] <= 0.0 || self.screen[1] <= 0.0 {
            return None;
        }
        let ndc = Vec2::new(px / self.screen[0] * 2.0 - 1.0, 1.0 - py / self.screen[1] * 2.0);
        let near = self.inv_view_proj.project_point3(ndc.extend(0.0));
        let far = self.inv_view_proj.project_point3(ndc.extend(1.0));
        let d = (far - near).normalize_or_zero();
        if d == Vec3::ZERO {
            return None;
        }
        let below = |t: f32| {
            let p = near + d * t;
            p.y <= self.height_at(p.x, p.z)
        };
        let step = (self.distance / 300.0).max(0.25);
        let max_t = self.distance * 6.0 + 1000.0;
        let mut t = 0.0;
        while t < max_t && !below(t) {
            t += step;
        }
        if t >= max_t {
            if d.y.abs() < 1e-6 {
                return None;
            }
            let t = (self.target_y - near.y) / d.y;
            return (t >= 0.0).then(|| near + d * t);
        }
        let (mut lo, mut hi) = ((t - step).max(0.0), t);
        for _ in 0..12 {
            let mid = 0.5 * (lo + hi);
            if below(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let p = near + d * hi;
        Some(Vec3::new(p.x, self.height_at(p.x, p.z), p.z))
    }
}

/// Every world view's [`PickView`] this frame, in draw order.
#[derive(Clone, Default)]
pub struct PickViews(pub Vec<PickView>);

impl PickViews {
    /// The view showing `camera`, else the first.
    fn of(&self, camera: Option<&str>) -> Option<&PickView> {
        camera.and_then(|c| self.0.iter().find(|v| v.camera.eq_ignore_ascii_case(c))).or(self.0.first())
    }

    /// The topmost view under interface point (x, y), else the first.
    fn at(&self, x: f32, y: f32) -> Option<&PickView> {
        self.0.iter().rev().find(|v| v.contains(x, y)).or(self.0.first())
    }
}

/// World view `view`'s pick view: the one of its camera.
fn pick_view(lua: &Lua, view: &LuaTable) -> Option<PickView> {
    let camera = super::controls::view_camera(lua, view);
    lua.app_data_ref::<PickViews>()?.of(camera.as_deref()).cloned()
}

fn vector(lua: &Lua, p: Vec3) -> LuaResult<shapes::Vector> {
    lua.globals().get::<LuaFunction>("Vector")?.call((p.x, p.y, p.z))
}

fn vector2(lua: &Lua, (x, y): (f32, f32)) -> LuaResult<shapes::Vector2> {
    lua.globals().get::<LuaFunction>("Vector2")?.call((x, y))
}

fn read_vector(v: &LuaTable) -> LuaResult<Vec3> {
    let get = |i: i64, k: &str| -> LuaResult<f32> { Ok(match v.raw_get::<Option<f32>>(i)? { Some(x) => x, None => v.get::<Option<f32>>(k)?.unwrap_or(0.0) }) };
    Ok(Vec3::new(get(1, "x")?, get(2, "y")?, get(3, "z")?))
}

/// World views' `Project(position)` / `ProjectMultiple(positions)` /
/// `GetScreenPos(unit)`: world points to interface coordinates from the
/// view's top left, through the view's own camera (FAF's reclaim labels,
/// off-screen indicators, notify overlay place controls with them).
pub fn register_view_methods(lua: &Lua) -> LuaResult<()> {
    lua_method!(lua, moho::UIWorldView,
        /// The interface coordinates of the world point `position`, from the view's top left.
        Project = |lua, (this, p): (Obj<moho::UIWorldView>, shapes::Vector)| {
            let at = read_vector(&p)?;
            let xy = pick_view(lua, &this).map_or((0.0, 0.0), |v| v.project(at));
            vector2(lua, xy)
        })?;
    lua_method!(lua, moho::UIWorldView,
        /// `Project` for each point of a table, keeping its keys.
        ProjectMultiple = |lua, (this, points): (Obj<moho::UIWorldView>, Keyed<shapes::Vector>)| {
            let out = lua.create_table()?;
            let v = pick_view(lua, &this);
            for pair in points.pairs::<LuaValue, LuaTable>() {
                let (k, p) = pair?;
                let xy = v.as_ref().map_or((0.0, 0.0), |v| v.project(read_vector(&p).unwrap_or_default()));
                out.raw_set(k, vector2(lua, xy)?)?;
            }
            Ok(Keyed::<shapes::Vector2>::new(out))
        })?;
    lua_method!(lua, moho::UIWorldView,
        /// The interface coordinates of a unit's position, from the view's top left; nil before the camera exists.
        GetScreenPos = |lua, (this, unit): (Obj<moho::UIWorldView>, Obj<moho::UserUnit>)| {
            let p: LuaTable = unit.call_method("GetPosition", ())?;
            let xy = match pick_view(lua, &this) {
                Some(v) => v.project(read_vector(&p)?),
                None => return Ok(None),
            };
            vector2(lua, xy).map(Some)
        })?;
    Ok(())
}

/// Replaces the `UnProject` / `GetMouseWorldPos` placeholders.
pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// `UnProject(view, {x, y})`: the ground under window point (x, y)
        /// through the world view's camera; nil when there's no camera yet or
        /// the ray misses the world.
        UnProject = |lua, (view, pos): (Loose<Obj<moho::UIWorldView>>, shapes::Vector)| {
            let (x, y) = (pos.get::<f32>(1)?, pos.get::<f32>(2)?);
            let hit = match view.into_inner() {
                LuaValue::Table(view) => pick_view(lua, &view),
                _ => lua.app_data_ref::<PickViews>().and_then(|v| v.of(None).cloned()),
            }
            .and_then(|v| v.ground(x, y));
            match hit {
                Some(p) => vector(lua, p).map(Some),
                None => Ok(None),
            }
        })?;
    lua_fn!(lua, global,
        /// Where the mouse points in the world, through the camera of the world
        /// view under it (the origin when unknown, as the placeholder answered).
        GetMouseWorldPos = |lua, ()| {
            let g = lua.globals();
            let (x, y) = (g.raw_get::<Option<f32>>("__engine_mouse_x")?.unwrap_or(0.0), g.raw_get::<Option<f32>>("__engine_mouse_y")?.unwrap_or(0.0));
            let hit = lua.app_data_ref::<PickViews>().and_then(|v| v.at(x, y)?.ground(x, y));
            vector(lua, hit.unwrap_or(Vec3::ZERO))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_down_finds_the_ground_below() {
        // A camera 50 above (10, 0, 20) looking straight down: the screen
        // centre is that point on flat ground at height 3.
        let eye = Vec3::new(10.0, 53.0, 20.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(10.0, 3.0, 20.0), Vec3::NEG_Z);
        let proj = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 1.0, 1000.0);
        let terrain = TerrainView { width: 64, height: 64, heights: vec![3.0; 65 * 65], water: None, map: None };
        let v = PickView { camera: "WorldCamera".into(), origin: [0.0, 0.0], view_proj: proj * view, inv_view_proj: (proj * view).inverse(), screen: [200.0, 200.0], ui_scale: 2.0, target_y: 3.0, distance: 50.0, terrain: Some(Arc::new(terrain)) };
        let p = v.ground(50.0, 50.0).unwrap();
        assert!((p - Vec3::new(10.0, 3.0, 20.0)).length() < 0.1, "{p}");
    }
}
