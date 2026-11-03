//! `Random([min,] [max])` — the sim's deterministic RNG. FAF also installs it
//! as `math.random` (`/lua/system/config.lua`), so it follows Lua semantics:
//! `Random()` → float in [0,1), `Random(n)` → int in [1,n], `Random(m,n)` →
//! int in [m,n]. Bounds are truncated toward zero like Lua 5.0's checkint.

use mlua::prelude::*;

use crate::engine::sim::rng::SimRng;
use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    if lua.app_data_ref::<SimRng>().is_none() {
        lua.set_app_data(SimRng::default());
    }
    lua_fn!(lua, global,
        /// The sim's deterministic RNG, with Lua semantics: `Random()` is a
        /// float in [0,1), `Random(n)` an integer in [1,n], `Random(m,n)` an
        /// integer in [m,n] (bounds truncated toward zero).
        Random = |lua, (a, b): (Option<f64>, Option<f64>)| {
            let mut rng = lua.app_data_mut::<SimRng>().expect("SimRng not in app_data");
            let (lo, hi) = match (a, b) {
                (None, _) => return Ok(rng.next_f64()),
                (Some(max), None) => (1, max.trunc() as i64),
                (Some(min), Some(max)) => (min.trunc() as i64, max.trunc() as i64),
            };
            if lo > hi {
                return Err(LuaError::runtime(format!("Random: interval is empty ({lo}, {hi})")));
            }
            Ok(rng.range_i64(lo, hi) as f64)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_semantics() {
        let lua = Lua::new();
        register(&lua).unwrap();
        let ok: bool = lua
            .load(
                "for i = 1, 200 do
                    local f = Random(); if f < 0 or f >= 1 then return false end
                    local a = Random(5); if a < 1 or a > 5 or a % 1 ~= 0 then return false end
                    local b = Random(-2, 2); if b < -2 or b > 2 then return false end
                 end
                 return true",
            )
            .eval()
            .unwrap();
        assert!(ok);
        assert!(lua.load("Random(3, 1)").exec().is_err());
    }
}
