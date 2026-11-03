pub mod scalar;
pub mod vector2;
pub mod vector_ops;

pub fn register(lua: &mlua::Lua) -> mlua::Result<()> {
    scalar::register(lua)?;
    vector2::register(lua)?;
    vector_ops::register(lua)?;
    Ok(())
}
