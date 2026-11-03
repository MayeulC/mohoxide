use mlua::prelude::*;

use crate::engine::sim::navigation::NavigatorConfig;
use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(NavigatorConfig::default());
    lua_fn!(lua, global,
        /// Sets the navigator's maximum personal position distance.
        SetNavigatorPersonalPosMaxDistance = |lua, distance: f64| {
            lua.app_data_mut::<NavigatorConfig>()
                .expect("NavigatorConfig not in app_data")
                .personal_pos_max_distance = distance;
            Ok(())
        })
}
