//! Moho's named colours: what a `Color` string may be instead of a hex code
//! (FAF lists them in `/engine/Enums.lua` and `/lua/shared/color.lua`), and
//! `EnumColorNames`.

use mlua::prelude::*;

use crate::lua_api::lua_fn;
use crate::lua_api::shapes::List;

/// Every named colour as `(name, ARGB)`, in moho's (alphabetical) order.
pub const NAMED: &[(&str, u32)] = &[
    ("AliceBlue", 0xFFF7FBFF),
    ("AntiqueWhite", 0xFFFFEBD6),
    ("Aqua", 0xFF00FFFF),
    ("Aquamarine", 0xFF7BFFD6),
    ("Azure", 0xFFF7FFFF),
    ("Beige", 0xFFF7F7DE),
    ("Bisque", 0xFFFFE7C6),
    ("Black", 0xFF000000),
    ("BlanchedAlmond", 0xFFFFEBCE),
    ("Blue", 0xFF0000FF),
    ("BlueViolet", 0xFF8C28E7),
    ("Brown", 0xFFA52829),
    ("BurlyWood", 0xFFDEBA84),
    ("CadetBlue", 0xFF5A9EA5),
    ("Chartreuse", 0xFF7BFF00),
    ("Chocolate", 0xFFD66918),
    ("Coral", 0xFFFF7D52),
    ("CornflowerBlue", 0xFF6396EF),
    ("Cornsilk", 0xFFFFFBDE),
    ("Crimson", 0xFFDE1439),
    ("Cyan", 0xFF00FFFF),
    ("DarkBlue", 0xFF00008C),
    ("DarkCyan", 0xFF008A8C),
    ("DarkGoldenrod", 0xFFBD8608),
    ("DarkGray", 0xFFADAAAD),
    ("DarkGreen", 0xFF006500),
    ("DarkKhaki", 0xFFBDB66B),
    ("DarkMagenta", 0xFF8C008C),
    ("DarkOliveGreen", 0xFF526929),
    ("DarkOrange", 0xFFFF8E00),
    ("DarkOrchid", 0xFF9C30CE),
    ("DarkRed", 0xFF8C0000),
    ("DarkSalmon", 0xFFEF967B),
    ("DarkSeaGreen", 0xFF8CBE8C),
    ("DarkSlateBlue", 0xFF4A3C8C),
    ("DarkSlateGray", 0xFF294D4A),
    ("DarkTurquoise", 0xFF00CFD6),
    ("DarkViolet", 0xFF9400D6),
    ("DeepPink", 0xFFFF1494),
    ("DeepSkyBlue", 0xFF00BEFF),
    ("DimGray", 0xFF6B696B),
    ("DodgerBlue", 0xFF1892FF),
    ("Firebrick", 0xFFB52021),
    ("FloralWhite", 0xFFFFFBF7),
    ("ForestGreen", 0xFF218A21),
    ("Fuchsia", 0xFFFF00FF),
    ("Gainsboro", 0xFFDEDFDE),
    ("GhostWhite", 0xFFFFFBFF),
    ("Gold", 0xFFFFD700),
    ("Goldenrod", 0xFFDEA621),
    ("Gray", 0xFF848284),
    ("Green", 0xFF008200),
    ("GreenYellow", 0xFFADFF29),
    ("Honeydew", 0xFFF7FFF7),
    ("HotPink", 0xFFFF69B5),
    ("IndianRed", 0xFFCE5D5A),
    ("Indigo", 0xFF4A0084),
    ("Ivory", 0xFFFFFFF7),
    ("Khaki", 0xFFF7E78C),
    ("Lavender", 0xFFE7E7FF),
    ("LavenderBlush", 0xFFFFF3F7),
    ("LawnGreen", 0xFF7BFF00),
    ("LemonChiffon", 0xFFFFFBCE),
    ("LightBlue", 0xFFADDBE7),
    ("LightCoral", 0xFFF78284),
    ("LightCyan", 0xFFE7FFFF),
    ("LightGoldenrodYellow", 0xFFFFFBD6),
    ("LightGray", 0xFFD6D3D6),
    ("LightGreen", 0xFF94EF94),
    ("LightPink", 0xFFFFB6C6),
    ("LightSalmon", 0xFFFFA27B),
    ("LightSeaGreen", 0xFF21B2AD),
    ("LightSkyBlue", 0xFF84CFFF),
    ("LightSlateGray", 0xFF738A9C),
    ("LightSteelBlue", 0xFFB5C7DE),
    ("LightYellow", 0xFFFFFFE7),
    ("Lime", 0xFF00FF00),
    ("LimeGreen", 0xFF31CF31),
    ("Linen", 0xFFFFF3E7),
    ("Magenta", 0xFFFF00FF),
    ("Maroon", 0xFF840000),
    ("MediumAquamarine", 0xFF63CFAD),
    ("MediumBlue", 0xFF0000CE),
    ("MediumOrchid", 0xFFBD55D6),
    ("MediumPurple", 0xFF9471DE),
    ("MediumSeaGreen", 0xFF39B273),
    ("MediumSlateBlue", 0xFF7B69EF),
    ("MediumSpringGreen", 0xFF00FB9C),
    ("MediumTurquoise", 0xFF4AD3CE),
    ("MediumVioletRed", 0xFFC61484),
    ("MidnightBlue", 0xFF181873),
    ("MintCream", 0xFFF7FFFF),
    ("MistyRose", 0xFFFFE7E7),
    ("Moccasin", 0xFFFFE7B5),
    ("NavajoWhite", 0xFFFFDFAD),
    ("Navy", 0xFF000084),
    ("OldLace", 0xFFFFF7E7),
    ("Olive", 0xFF848200),
    ("OliveDrab", 0xFF6B8E21),
    ("Orange", 0xFFFFA600),
    ("OrangeRed", 0xFFFF4500),
    ("Orchid", 0xFFDE71D6),
    ("PaleGoldenrod", 0xFFEFEBAD),
    ("PaleGreen", 0xFF9CFB9C),
    ("PaleTurquoise", 0xFFADEFEF),
    ("PaleVioletRed", 0xFFDE7194),
    ("PapayaWhip", 0xFFFFEFD6),
    ("PeachPuff", 0xFFFFDBBD),
    ("Peru", 0xFFCE8639),
    ("Pink", 0xFFFFC3CE),
    ("Plum", 0xFFDEA2DE),
    ("PowderBlue", 0xFFB5E3E7),
    ("Purple", 0xFF840084),
    ("Red", 0xFFFF0000),
    ("RosyBrown", 0xFFBD8E8C),
    ("RoyalBlue", 0xFF4269E7),
    ("SaddleBrown", 0xFF8C4510),
    ("Salmon", 0xFFFF8273),
    ("SandyBrown", 0xFFF7A663),
    ("SeaGreen", 0xFF298A52),
    ("SeaShell", 0xFFFFF7EF),
    ("Sienna", 0xFFA55129),
    ("Silver", 0xFFC6C3C6),
    ("SkyBlue", 0xFF84CFEF),
    ("SlateBlue", 0xFF6B59CE),
    ("SlateGray", 0xFF738294),
    ("Snow", 0xFFFFFBFF),
    ("SpringGreen", 0xFF00FF7B),
    ("SteelBlue", 0xFF4282B5),
    ("Tan", 0xFFD6B68C),
    ("Teal", 0xFF008284),
    ("Thistle", 0xFFDEBEDE),
    ("Tomato", 0xFFFF6142),
    ("Turquoise", 0xFF42E3D6),
    ("Violet", 0xFFEF82EF),
    ("Wheat", 0xFFF7DFB5),
    ("White", 0xFFFFFFFF),
    ("WhiteSmoke", 0xFFF7F7F7),
    ("Yellow", 0xFFFFFF00),
    ("YellowGreen", 0xFF9CCF31),
    ("transparent", 0x00000000),
];

/// The ARGB of a colour name (any case), if moho knows it.
pub fn named(name: &str) -> Option<u32> {
    NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, argb)| argb)
}

/// Moho's `SCR_DecodeColor`: a colour name, or hex `"RRGGBB"` / `"AARRGGBB"`
/// (a leading `#` allowed), as ARGB.
pub fn decode(s: &str) -> Option<u32> {
    if let Some(argb) = named(s) {
        return Some(argb);
    }
    let hex = s.trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        8 => Some(v),
        6 => Some(0xFF00_0000 | v),
        _ => None,
    }
}

/// Our colour for an army the session gave no colour (index 1-based; 0 none).
pub fn fallback_army_color(army: u32) -> u32 {
    const C: [u32; 8] = [0x3373F2, 0xE64033, 0xF2CC26, 0x40CC59, 0xB34DD9, 0xF28C26, 0x4DD9D9, 0xE673B3];
    if army == 0 {
        return 0xFF8C_8C8C;
    }
    0xFF00_0000 | C[(army as usize - 1) % C.len()]
}

/// ARGB as RGB in 0..1.
pub fn rgb(argb: u32) -> [f32; 3] {
    let c = |shift: u32| ((argb >> shift) & 0xFF) as f32 / 255.0;
    [c(16), c(8), c(0)]
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// The named colours, in moho's (alphabetical) order.
        EnumColorNames = |lua, ()| lua.create_sequence_from(NAMED.iter().map(|&(n, _)| n)).map(List::<String>::new))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn enum_color_names_lists_every_name_in_order() {
        let lua = mlua::Lua::new();
        super::register(&lua).unwrap();
        let r: (usize, String, String) = lua.load("local t = EnumColorNames() return #t, t[1], t[#t]").eval().unwrap();
        assert_eq!(r, (141, "AliceBlue".to_string(), "transparent".to_string()));
    }

    #[test]
    fn decode_names_and_hex() {
        assert_eq!(super::decode("White"), Some(0xFFFFFFFF));
        assert_eq!(super::decode("FFe80a0a"), Some(0xFFE80A0A));
        assert_eq!(super::decode("80ff00"), Some(0xFF80FF00));
        assert_eq!(super::decode("#00000080"), Some(0x80));
        assert_eq!(super::decode("fff"), None);
    }

    #[test]
    fn named_is_case_insensitive() {
        assert_eq!(super::named("gray"), Some(0xFF848284));
        assert_eq!(super::named("Transparent"), Some(0));
        assert_eq!(super::named("nope"), None);
    }
}
