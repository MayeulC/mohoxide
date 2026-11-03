use std::fmt;
use mlua::prelude::*;

pub struct PrettyLua(pub LuaValue);

impl fmt::Display for PrettyLua {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_value(f, &self.0, 0)
    }
}

fn fmt_value(f: &mut fmt::Formatter<'_>, val: &LuaValue, depth: usize) -> fmt::Result {
    let pad = "  ".repeat(depth);
    match val {
        LuaValue::Table(t) => {
            writeln!(f, "{{")?;
            for (k, v) in t.pairs::<LuaValue, LuaValue>().flatten() {
                write!(f, "{}  [{}] = ", pad, PrettyLua(k))?;
                fmt_value(f, &v, depth + 1)?;
                writeln!(f, ",")?;
            }
            write!(f, "{}}}", pad)
        }
        LuaValue::String(s) => write!(f, "{:?}", s.to_str().as_deref().unwrap_or("?")),
        LuaValue::Integer(i) => write!(f, "{i}"),
        LuaValue::Number(n) => write!(f, "{n}"),
        LuaValue::Boolean(b) => write!(f, "{b}"),
        LuaValue::Nil => write!(f, "nil"),
        other => write!(f, "{other:?}"),
    }
}
