//! Audio globals. Audio is client-side presentation and must never influence
//! the sim, so these only record state for the (future) audio system.

use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::lua_api::lua_fn;

/// The language selected via `AudioSetLanguage`.
#[derive(Default, Debug, Clone)]
pub struct AudioLanguage(pub String);

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(AudioLanguage::default());
    lua_fn!(lua, global,
        /// True if voice-over banks exist for `language` (`/sounds/voice/<lang>/`).
        HasLocalizedVO = |lua, language: String| {
            let vfs = lua
                .app_data_ref::<Arc<RwLock<Vfs>>>()
                .expect("VFS not in app_data")
                .clone();
            let info = vfs
                .read()
                .expect("VFS poisoned")
                .file_info(&format!("/sounds/voice/{language}"));
            Ok(info.is_some_and(|i| i.is_folder))
        })?;
    lua_fn!(lua, global,
        /// Records the language selected for audio (`AudioLanguage`).
        AudioSetLanguage = |lua, language: String| {
            lua.set_app_data(AudioLanguage(language));
            Ok(())
        })
}
