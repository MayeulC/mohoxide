//! Lua values carried from a client to the sim (a `SimCallback`'s `Args`).
//!
//! Moho serializes the interface's Lua table into the command stream, and
//! every peer rebuilds it in its sim. Here it's a plain value tree: nil,
//! booleans, numbers (as raw `f64` bits, so they arrive exactly), strings and
//! tables (raw fields only, metatables dropped: a `Vector` arrives as `{x, y,
//! z}`, which is how FAF's callbacks index it). Functions, userdata, threads
//! and cycles can't cross and are left out. Table fields are kept in one
//! canonical key order, so the sim builds the same table (and `pairs` order)
//! on every machine whatever order the sender's VM held them in.

use std::cmp::Ordering;

use mlua::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LuaData {
    Nil,
    Bool(bool),
    /// `f64` bits.
    Number(u64),
    String(Vec<u8>),
    /// Fields in canonical key order ([`key_order`]).
    Table(Vec<(LuaData, LuaData)>),
}

/// Tables nested deeper than this are cut off (left out).
const MAX_DEPTH: usize = 64;

impl LuaData {
    pub fn number(n: f64) -> Self {
        LuaData::Number(n.to_bits())
    }

    pub fn string(s: &str) -> Self {
        LuaData::String(s.as_bytes().to_vec())
    }

    /// `value` as it can cross to the sim (`Nil` when it can't at all).
    pub fn from_lua(value: &LuaValue) -> Self {
        convert(value, &mut Vec::new()).unwrap_or(LuaData::Nil)
    }

    pub fn to_lua(&self, lua: &Lua) -> LuaResult<LuaValue> {
        Ok(match self {
            LuaData::Nil => LuaValue::Nil,
            LuaData::Bool(b) => LuaValue::Boolean(*b),
            LuaData::Number(bits) => LuaValue::Number(f64::from_bits(*bits)),
            LuaData::String(s) => LuaValue::String(lua.create_string(s)?),
            LuaData::Table(fields) => {
                let array = fields.iter().take_while(|(k, _)| matches!(k, LuaData::Number(_))).count();
                let t = lua.create_table_with_capacity(array, fields.len() - array)?;
                for (k, v) in fields {
                    t.raw_set(k.to_lua(lua)?, v.to_lua(lua)?)?;
                }
                LuaValue::Table(t)
            }
        })
    }

    /// Compact bytes (the wire / replay form).
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            LuaData::Nil => out.push(0),
            LuaData::Bool(false) => out.push(1),
            LuaData::Bool(true) => out.push(2),
            LuaData::Number(bits) => {
                out.push(3);
                out.extend_from_slice(&bits.to_le_bytes());
            }
            LuaData::String(s) => {
                out.push(4);
                out.extend_from_slice(&(s.len() as u32).to_le_bytes());
                out.extend_from_slice(s);
            }
            LuaData::Table(fields) => {
                out.push(5);
                out.extend_from_slice(&(fields.len() as u32).to_le_bytes());
                for (k, v) in fields {
                    k.encode(out);
                    v.encode(out);
                }
            }
        }
    }

    /// Reads one [`Self::encode`]d value from the front of `bytes`.
    pub fn decode(bytes: &mut &[u8]) -> Result<Self, String> {
        Self::decode_at(bytes, 0)
    }

    fn decode_at(bytes: &mut &[u8], depth: usize) -> Result<Self, String> {
        if depth > MAX_DEPTH {
            return Err("Lua data nested too deep".into());
        }
        let tag = take(bytes, 1)?[0];
        Ok(match tag {
            0 => LuaData::Nil,
            1 => LuaData::Bool(false),
            2 => LuaData::Bool(true),
            3 => LuaData::Number(u64::from_le_bytes(take(bytes, 8)?.try_into().unwrap())),
            4 => {
                let n = read_len(bytes)?;
                LuaData::String(take(bytes, n)?.to_vec())
            }
            5 => {
                let n = read_len(bytes)?;
                // Each field takes at least two bytes: don't trust `n` to size it.
                let mut fields = Vec::with_capacity(n.min(bytes.len() / 2));
                for _ in 0..n {
                    let k = Self::decode_at(bytes, depth + 1)?;
                    let v = Self::decode_at(bytes, depth + 1)?;
                    fields.push((k, v));
                }
                LuaData::Table(fields)
            }
            t => return Err(format!("bad Lua data tag {t}")),
        })
    }
}

fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if bytes.len() < n {
        return Err("Lua data cut short".into());
    }
    let (head, rest) = bytes.split_at(n);
    *bytes = rest;
    Ok(head)
}

fn read_len(bytes: &mut &[u8]) -> Result<usize, String> {
    Ok(u32::from_le_bytes(take(bytes, 4)?.try_into().unwrap()) as usize)
}

/// `path`: the tables being converted (a table met again inside itself is a
/// cycle, and is left out).
fn convert(value: &LuaValue, path: &mut Vec<*const std::ffi::c_void>) -> Option<LuaData> {
    match value {
        LuaValue::Nil => Some(LuaData::Nil),
        LuaValue::Boolean(b) => Some(LuaData::Bool(*b)),
        LuaValue::Integer(i) => Some(LuaData::number(*i as f64)),
        LuaValue::Number(n) => Some(LuaData::number(*n)),
        LuaValue::String(s) => Some(LuaData::String(s.as_bytes().to_vec())),
        LuaValue::Table(t) => {
            let ptr = t.to_pointer();
            if path.len() >= MAX_DEPTH || path.contains(&ptr) {
                return None;
            }
            path.push(ptr);
            let mut fields = Vec::new();
            for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                let Ok((k, v)) = pair else { continue };
                let (Some(k), Some(v)) = (convert(&k, path), convert(&v, path)) else { continue };
                if v != LuaData::Nil {
                    fields.push((k, v));
                }
            }
            path.pop();
            fields.sort_by(|a, b| key_order(&a.0, &b.0));
            Some(LuaData::Table(fields))
        }
        _ => None,
    }
}

/// Canonical field order: numbers (by value), then booleans, strings
/// (bytewise), tables (by encoding). Numbers first so an array's elements
/// come out 1, 2, 3, ...
pub fn key_order(a: &LuaData, b: &LuaData) -> Ordering {
    fn rank(d: &LuaData) -> u8 {
        match d {
            LuaData::Number(_) => 0,
            LuaData::Bool(_) => 1,
            LuaData::String(_) => 2,
            LuaData::Table(_) => 3,
            LuaData::Nil => 4,
        }
    }
    match (a, b) {
        (LuaData::Number(x), LuaData::Number(y)) => f64::from_bits(*x).total_cmp(&f64::from_bits(*y)),
        (LuaData::Bool(x), LuaData::Bool(y)) => x.cmp(y),
        (LuaData::String(x), LuaData::String(y)) => x.cmp(y),
        (LuaData::Table(_), LuaData::Table(_)) => {
            let (mut x, mut y) = (Vec::new(), Vec::new());
            a.encode(&mut x);
            b.encode(&mut y);
            x.cmp(&y)
        }
        _ => rank(a).cmp(&rank(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_cross_in_canonical_order_and_round_trip() {
        let lua = Lua::new();
        let v: LuaValue = lua
            .load(
                r#"
                local t = { Func = 'x', 10, 20, z = { 1.5, -0.25, 1e300 }, flag = true, [0.5] = 'half', f = print }
                t.self = t
                return t
                "#,
            )
            .eval()
            .unwrap();
        let d = LuaData::from_lua(&v);
        let LuaData::Table(fields) = &d else { panic!("{d:?}") };
        let keys: Vec<&LuaData> = fields.iter().map(|(k, _)| k).collect();
        // Numbers first by value, then strings; the function and the cycle are left out.
        assert_eq!(
            keys,
            [&LuaData::number(0.5), &LuaData::number(1.0), &LuaData::number(2.0), &LuaData::string("Func"), &LuaData::string("flag"), &LuaData::string("z")]
        );
        let mut bytes = Vec::new();
        d.encode(&mut bytes);
        let back = LuaData::decode(&mut bytes.as_slice()).unwrap();
        assert_eq!(back, d);
        // Rebuilt in a sim VM, the numbers are bit-exact.
        let t: LuaTable = match back.to_lua(&lua).unwrap() {
            LuaValue::Table(t) => t,
            other => panic!("{other:?}"),
        };
        let z: LuaTable = t.get("z").unwrap();
        assert_eq!(z.get::<f64>(3).unwrap().to_bits(), 1e300f64.to_bits());
        assert_eq!(t.get::<f64>(2).unwrap(), 20.0);
    }

    #[test]
    fn bad_bytes_are_an_error() {
        assert!(LuaData::decode(&mut [9u8].as_slice()).is_err());
        assert!(LuaData::decode(&mut [4u8, 10, 0, 0, 0, b'a'].as_slice()).is_err());
        assert!(LuaData::decode(&mut [5u8, 255, 255, 255, 255].as_slice()).is_err());
    }
}
