//! The mouse cursor (moho's `CMauiCursor`): FAF picks its texture (the
//! command mode's cursor over the world, the default elsewhere) and the
//! renderer makes it the window's hardware cursor (drawn in the frame it
//! would trail the mouse by a frame or more).
//!
//! Each interface frame the world view under the mouse gets
//! `OnUpdateCursor` (moho calls it every frame), where FAF works out the
//! cursor for the command mode and the keys held. Render-thread only.

use mlua::prelude::*;

use crate::engine::api::moho;
use crate::lua_api::{Obj, lua_method};

/// A cursor texture and its hotspot (pixels from its top-left).
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub path: String,
    pub hotspot: [f32; 2],
}

/// The cursor's state (app data of the user VM).
#[derive(Default)]
struct Cursor {
    default: Option<Texture>,
    current: Option<Texture>,
    hidden: bool,
}

fn state(lua: &Lua) -> mlua::AppDataRefMut<'_, Cursor> {
    lua.app_data_mut::<Cursor>().expect("Cursor not in app_data")
}

fn texture(path: Option<String>, x: Option<f32>, y: Option<f32>) -> Option<Texture> {
    path.filter(|p| !p.is_empty()).map(|path| Texture { path, hotspot: [x.unwrap_or(0.0), y.unwrap_or(0.0)] })
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Cursor::default());
    lua_method!(lua, moho::Cursor,
        /// The texture shown when nothing else is set (`Reset` goes back to it).
        /// An empty or nil path clears it; the hotspot defaults to (0, 0).
        SetDefaultTexture = |lua, (_this, path, x, y): (Obj<moho::Cursor>, Option<String>, Option<f32>, Option<f32>)| {
            state(lua).default = texture(path, x, y);
            Ok(())
        })?;
    lua_method!(lua, moho::Cursor,
        /// Shows this texture. A nil texture leaves the cursor as it is (FAF's
        /// animated cursors set each frame's file; `ResetToDefault` goes back
        /// to the default).
        SetNewTexture = |lua, (_this, path, x, y): (Obj<moho::Cursor>, Option<String>, Option<f32>, Option<f32>)| {
            if let Some(t) = texture(path, x, y) {
                state(lua).current = Some(t);
            }
            Ok(())
        })?;
    lua_method!(lua, moho::Cursor,
        /// Goes back to the default texture.
        ResetToDefault = |lua, _this: Obj<moho::Cursor>| {
            let mut s = state(lua);
            s.current = s.default.clone();
            Ok(())
        })?;
    lua_method!(lua, moho::Cursor,
        /// Shows the cursor.
        Show = |lua, _this: Obj<moho::Cursor>| {
            state(lua).hidden = false;
            Ok(())
        })?;
    lua_method!(lua, moho::Cursor,
        /// Hides the cursor (cinematics).
        Hide = |lua, _this: Obj<moho::Cursor>| {
            state(lua).hidden = true;
            Ok(())
        })?;
    Ok(())
}

/// Lets the world view under the mouse pick the cursor (`OnUpdateCursor`).
/// The view is the one the last mouse event found (hit testing the whole
/// tree each frame costs more than the rest of the interface frame).
pub fn update(lua: &Lua) -> LuaResult<()> {
    let Some(view) = crate::ui::controls::hovered_world_view(lua)? else { return Ok(()) };
    if let Some(f) = view.get::<Option<LuaFunction>>("OnUpdateCursor")?
        && let Err(e) = f.call::<()>(view) {
        tracing::warn!(target: "lua", "OnUpdateCursor failed: {e}");
    }
    Ok(())
}

/// The cursor FAF wants shown.
#[derive(Clone, Debug, PartialEq)]
pub enum Wanted {
    /// No cursor set yet: the system's.
    System,
    /// `Hide` (cinematics): none.
    Hidden,
    Texture(Texture),
}

pub fn wanted(lua: &Lua) -> Wanted {
    let s = state(lua);
    match (&s.current, s.hidden) {
        (_, true) => Wanted::Hidden,
        (Some(t), false) => Wanted::Texture(t.clone()),
        (None, false) => Wanted::System,
    }
}
