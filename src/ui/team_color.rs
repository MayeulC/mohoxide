//! `TeamColorMode`: the player's own army colours, drawn instead of the
//! session's (FAF's team colour button, `ui/game/multifunction.lua`). Moho's
//! takes a bool, the mode on or off; FAF's binary patch
//! (FA-Binary-Patches `section/TeamColorMode.cpp`) also takes a string of hex
//! colours, `"ffrrggbb,ffrrggbb,..."`, the first for army 1, which the
//! renderer's army colour lookup reads while the mode is on. FAF sets the
//! colours, then turns the mode on. An army without one gets moho's own
//! team colour: `GameColors.TeamColorMode`'s `Self` / `Ally` / `Enemy` /
//! `Neutral` by its relation to the focus army when the mode is turned on.
//! Interface-only: the sim never sees it.

use mlua::prelude::*;

use mlua::Either;

use crate::lua_api::lua_fn;
use crate::ui::shapes::Loose;

/// The interface's team colour state (app data in the user Lua state).
#[derive(Default)]
pub struct TeamColors {
    pub on: bool,
    /// ARGB per army from `TeamColorMode(string)`, index = army - 1.
    pub colors: Vec<u32>,
    /// ARGB per army by relation (`GameColors.TeamColorMode`), from when
    /// the mode was last turned on; index = army - 1.
    pub relation: Vec<u32>,
    /// Bumped on every call, so the frame redraws the colours only then.
    pub version: u64,
}

/// The patch's parse: hex digits make a colour, any other character ends
/// one (consecutive separators give 0), a trailing separator adds nothing.
/// Overwrites the first colours only, as the patch's fixed table does.
fn parse_into(colors: &mut Vec<u32>, s: &str) {
    let mut parts: Vec<&str> = s.split(|c: char| !c.is_ascii_hexdigit()).collect();
    if parts.last() == Some(&"") {
        parts.pop();
    }
    for (i, part) in parts.into_iter().enumerate() {
        let v = part.chars().fold(0u32, |acc, c| (acc << 4) | c.to_digit(16).unwrap_or(0));
        match colors.get_mut(i) {
            Some(slot) => *slot = v,
            None => colors.push(v),
        }
    }
}

/// Each army's `GameColors.TeamColorMode` colour by its relation to the
/// focus army (an observer's: every army neutral).
const RELATION_SRC: &str = r#"
local colors = import('/lua/gamecolors.lua').GameColors.TeamColorMode
local focus = GetFocusArmy()
local out = {}
for index, army in GetArmiesTable().armiesTable do
    local c
    if index == focus then c = colors.Self
    elseif focus == -1 or army.civilian or IsNeutral(focus, index) then c = colors.Neutral
    elseif IsAlly(focus, index) then c = colors.Ally
    else c = colors.Enemy end
    out[index] = c
end
return out
"#;

fn relation_colors(lua: &Lua) -> LuaResult<Vec<u32>> {
    let t: LuaTable = lua.load(RELATION_SRC).set_name("=[engine TeamColorMode]").eval()?;
    let n = t.raw_len();
    (1..=n).map(|i| Ok(t.raw_get::<Option<String>>(i)?.as_deref().and_then(crate::engine::api::colors::decode).unwrap_or(0))).collect()
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(TeamColors::default());
    lua_fn!(lua, global,
        /// `TeamColorMode(true|false)` turns team colours by relation on or off;
        /// `TeamColorMode("<colour string>")` sets armies' colours.
        TeamColorMode = |lua, mode: Loose<Either<bool, String>>| {
            let mode = mode.into_inner();
            // Before borrowing the state: this runs Lua.
            let relation = match mode {
                LuaValue::Boolean(true) => Some(relation_colors(lua).unwrap_or_else(|e| {
                    tracing::warn!("TeamColorMode: team colours by relation: {e}");
                    Vec::new()
                })),
                _ => None,
            };
            let mut t = lua.app_data_mut::<TeamColors>().expect("TeamColors not in app_data");
            if let Some(r) = relation {
                t.relation = r;
            }
            match mode {
                LuaValue::Boolean(on) => t.on = on,
                LuaValue::String(s) => parse_into(&mut t.colors, &s.to_str()?),
                other => return Err(LuaError::runtime(format!("TeamColorMode: expected a boolean or a colour string, got {}", other.type_name()))),
            }
            t.version += 1;
            Ok(())
        })?;
    Ok(())
}

/// The team colour state's version, and while the mode is on each army's
/// colour (index = army - 1): its `TeamColorMode` string colour, else its
/// colour by relation.
pub fn current(lua: &Lua) -> (u64, Option<Vec<u32>>) {
    lua.app_data_ref::<TeamColors>().map_or((0, None), |t| {
        let colors = t.on.then(|| (0..t.colors.len().max(t.relation.len())).map(|i| t.colors.get(i).or(t.relation.get(i)).copied().unwrap_or(0)).collect());
        (t.version, colors)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn colours_then_mode() {
        let lua = mlua::Lua::new();
        super::register(&lua).unwrap();
        lua.load("TeamColorMode('ff436eee,FFE80A0A,')").exec().unwrap();
        assert_eq!(super::current(&lua).1, None);
        lua.load("TeamColorMode(true)").exec().unwrap();
        assert_eq!(super::current(&lua).1, Some(vec![0xFF436EEE, 0xFFE80A0A]));
        // A shorter list overwrites the first colours only.
        lua.load("TeamColorMode('ff000000')").exec().unwrap();
        assert_eq!(super::current(&lua).1, Some(vec![0xFF000000, 0xFFE80A0A]));
        lua.load("TeamColorMode(false)").exec().unwrap();
        let (version, colors) = super::current(&lua);
        assert_eq!((version, colors), (4, None));
    }

    /// Without colours of its own, the mode colours armies by relation to
    /// the focus army.
    #[test]
    fn relation_colours_fill_in() {
        let lua = mlua::Lua::new();
        crate::engine::api::colors::register(&lua).unwrap();
        super::register(&lua).unwrap();
        lua.load(
            r#"
            function import() return { GameColors = { TeamColorMode = { Self = 'RoyalBlue', Enemy = 'FFE80A0A', Ally = 'DarkGreen', Neutral = 'Goldenrod' } } } end
            function GetFocusArmy() return 1 end
            function GetArmiesTable() return { armiesTable = { {}, {}, {}, { civilian = true } } } end
            function IsNeutral(a, b) return false end
            function IsAlly(a, b) return b == 2 end
            TeamColorMode(true)
            "#,
        )
        .exec()
        .unwrap();
        let named = |n: &str| crate::engine::api::colors::named(n).unwrap();
        assert_eq!(super::current(&lua).1, Some(vec![named("RoyalBlue"), named("DarkGreen"), 0xFFE80A0A, named("Goldenrod")]));
        // String colours win over them.
        lua.load("TeamColorMode('ff000000')").exec().unwrap();
        assert_eq!(super::current(&lua).1.unwrap()[0], 0xFF000000);
    }

    #[test]
    fn parse_like_the_patch() {
        let mut c = Vec::new();
        super::parse_into(&mut c, "ff112233,,ff445566");
        assert_eq!(c, vec![0xFF112233, 0, 0xFF445566]);
        let mut c = Vec::new();
        super::parse_into(&mut c, "");
        assert!(c.is_empty());
    }
}
