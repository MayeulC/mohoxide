//! Keyframes: the whole sim at the end of a tick, saved so a replay can
//! jump back to it and, later, as save games.
//!
//! A keyframe holds the sim VM's heap ([`heap_snapshot`]), the payloads of
//! its engine userdata, and the engine's own sim state. It loads into an
//! engine that started the same session the same way and stopped at the
//! base: the end of game start-up, before the first tick
//! ([`mark_base`]). Start-up is deterministic, so everything made up to
//! there (blueprints, classes, the map's props) is the same object with the
//! same gcid in both, and the keyframe only carries what changed since.
//!
//! Keyframes are taken at the end of a collection tick, when the heap holds
//! only live objects (`sim::gc::EVERY`).

use mlua::prelude::*;

use crate::engine::api;
pub mod recorder;
mod state;
pub mod store;

use crate::engine::lua::heap_snapshot::{self, Base};

/// The VM's own objects end here (`heap_snapshot::prepare`).
#[derive(Clone, Copy, Debug)]
struct Internal(u32);

/// The base of this session's sim VM ([`mark_base`]).
#[derive(Clone, Copy, Debug)]
pub struct SessionBase(pub Base);

/// Readies the sim VM for keyframes once the engine's natives are
/// registered, before any script runs.
///
/// mlua makes a userdata type's metatable the first time one is made; one
/// made after the base would, after a load, be made again with a new gcid
/// (and its methods as runtime closures a keyframe can't hold). So one of
/// each is made here.
pub fn prepare(lua: &Lua) -> LuaResult<()> {
    for kind in 0..UserdataKind::COUNT {
        let ud = UserdataKind::from_tag(kind).expect("kind").sample(lua)?;
        drop(ud);
    }
    let internal = heap_snapshot::prepare(lua)?;
    lua.set_app_data(Internal(internal));
    Ok(())
}

/// Marks the base: call at the end of game start-up, before the first tick.
pub fn mark_base(lua: &Lua) -> LuaResult<()> {
    let internal = lua.app_data_ref::<Internal>().map(|i| i.0).ok_or_else(|| LuaError::runtime("keyframes: VM not prepared"))?;
    let base = Base::mark(lua, internal)?;
    lua.set_app_data(SessionBase(base));
    Ok(())
}

/// The sim at the end of tick `tick`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Keyframe {
    pub tick: u64,
    pub base: Option<(u32, u32, u32)>,
    pub heap: Vec<u8>,
    /// The engine userdata newer than the base: `(gcid, kind, payload)`.
    pub userdata: Vec<u8>,
    /// The engine's own sim state, by name (`state`).
    pub state: std::collections::BTreeMap<String, Vec<u8>>,
}

/// Saves the sim held by `lua` (the end of a collection tick).
pub fn save(lua: &Lua, tick: u64) -> LuaResult<Keyframe> {
    let base = lua.app_data_ref::<SessionBase>().map(|b| b.0).ok_or_else(|| LuaError::runtime("keyframes: no base marked"))?;
    let mut userdata = Vec::new();
    let mut save_ud = |gcid: u32, ud: LuaAnyUserData| -> LuaResult<Vec<u8>> {
        let (kind, payload) = UserdataKind::save(&ud)?;
        put_var(&mut userdata, gcid as u64);
        userdata.push(kind as u8);
        put_var(&mut userdata, payload.len() as u64);
        userdata.extend_from_slice(&payload);
        Ok(Vec::new())
    };
    let heap = heap_snapshot::save(lua, &base, Some(&mut save_ud))?.heap;
    let state = state::save(lua)?;
    Ok(Keyframe { tick, base: Some((base.gcid, base.proto, base.internal)), heap, userdata, state })
}

/// Whether `keyframe` loads into `lua`: saved from a session started the
/// same way (same build, session and start-up).
pub fn fits(lua: &Lua, keyframe: &Keyframe) -> bool {
    lua.app_data_ref::<SessionBase>().is_some_and(|b| keyframe.base == Some((b.0.gcid, b.0.proto, b.0.internal)))
}

/// Loads `keyframe` into `lua`, an engine stopped at its base: the sim
/// carries on from the end of its tick.
pub fn load(lua: &Lua, keyframe: &Keyframe) -> LuaResult<()> {
    let base = lua.app_data_ref::<SessionBase>().map(|b| b.0).ok_or_else(|| LuaError::runtime("keyframes: no base marked"))?;
    if keyframe.base != Some((base.gcid, base.proto, base.internal)) {
        return Err(LuaError::runtime(format!("keyframe base {:?} differs from this session's {base:?} (another build or session?)", keyframe.base)));
    }
    let mut payloads = std::collections::BTreeMap::new();
    let mut r = &keyframe.userdata[..];
    while !r.is_empty() {
        let gcid = get_var(&mut r)? as u32;
        let kind = UserdataKind::from_tag(take(&mut r, 1)?[0]).ok_or_else(|| LuaError::runtime("keyframe: unknown userdata kind"))?;
        let len = get_var(&mut r)? as usize;
        payloads.insert(gcid, (kind, take(&mut r, len)?.to_vec()));
    }
    let loaded = heap_snapshot::load(lua, &keyframe.heap, |gcid| {
        let (kind, payload) = payloads.get(&gcid).ok_or_else(|| LuaError::runtime(format!("keyframe: no payload for userdata gcid {gcid}")))?;
        kind.load(lua, payload)
    })?;
    state::load(lua, &keyframe.state, &loaded)?;
    drop(loaded);
    // Registry tables cached before the load may have been replaced.
    api::script_object::clear_registry_cache(lua);
    // The saved game had just collected (keyframes are taken on collection
    // ticks): so does this one, dropping what its own start-up made that the
    // game had since let go.
    heap_snapshot::collect(lua)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Engine userdata

/// The sim's userdata types, by the tag a keyframe stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum UserdataKind {
    Army,
    Command,
    Decal,
    EconomyEvent,
    Effect,
    Category,
    Entity,
    Manipulator,
    Personality,
    Platoon,
    ScriptTask,
    Sound,
    Weapon,
}

impl UserdataKind {
    const COUNT: u8 = 13;

    fn from_tag(tag: u8) -> Option<Self> {
        use UserdataKind::*;
        Some(match tag {
            0 => Army,
            1 => Command,
            2 => Decal,
            3 => EconomyEvent,
            4 => Effect,
            5 => Category,
            6 => Entity,
            7 => Manipulator,
            8 => Personality,
            9 => Platoon,
            10 => ScriptTask,
            11 => Sound,
            12 => Weapon,
            _ => return None,
        })
    }

    /// One of this kind (to make mlua's metatable for it).
    fn sample(self, lua: &Lua) -> LuaResult<LuaAnyUserData> {
        self.load(lua, &self.sample_payload())
    }

    fn sample_payload(self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            UserdataKind::Category => put_var(&mut out, 0),
            UserdataKind::Sound => {
                put_str(&mut out, "");
                put_str(&mut out, "");
                out.push(0);
            }
            UserdataKind::Weapon => {
                put_var(&mut out, 0);
                put_var(&mut out, 0);
            }
            _ => put_var(&mut out, 0),
        }
        out
    }

    fn save(ud: &LuaAnyUserData) -> LuaResult<(Self, Vec<u8>)> {
        use crate::engine::api::*;
        let mut out = Vec::new();
        macro_rules! id {
            ($ty:ty, $kind:expr) => {
                if let Ok(h) = ud.borrow::<$ty>() {
                    put_var(&mut out, h.0 as u64);
                    return Ok(($kind, out));
                }
            };
        }
        id!(army::ArmyHandle, UserdataKind::Army);
        id!(command::CommandHandle, UserdataKind::Command);
        id!(presentation::DecalHandle, UserdataKind::Decal);
        id!(economy_event::EconomyEventHandle, UserdataKind::EconomyEvent);
        id!(effect::EffectHandle, UserdataKind::Effect);
        id!(entity::EntityHandle, UserdataKind::Entity);
        id!(manipulator::ManipulatorHandle, UserdataKind::Manipulator);
        id!(personality::PersonalityHandle, UserdataKind::Personality);
        id!(platoon::PlatoonHandle, UserdataKind::Platoon);
        id!(script_task::ScriptTaskHandle, UserdataKind::ScriptTask);
        if let Ok(c) = ud.borrow::<categories::EntityCategory>() {
            let words = c.0.words();
            put_var(&mut out, words.len() as u64);
            for w in words {
                out.extend_from_slice(&w.to_le_bytes());
            }
            return Ok((UserdataKind::Category, out));
        }
        if let Ok(s) = ud.borrow::<sound::SoundHandle>() {
            put_str(&mut out, &s.bank);
            put_str(&mut out, &s.cue);
            match &s.lod_cutoff {
                Some(l) => {
                    out.push(1);
                    put_str(&mut out, l);
                }
                None => out.push(0),
            }
            return Ok((UserdataKind::Sound, out));
        }
        if let Ok(w) = ud.borrow::<weapon::WeaponHandle>() {
            put_var(&mut out, w.unit as u64);
            put_var(&mut out, w.index as u64);
            return Ok((UserdataKind::Weapon, out));
        }
        let keys: Vec<String> = ud
            .metatable()
            .map(|mt| mt.pairs::<LuaValue>().filter_map(|p| p.ok()).map(|(k, _)| format!("{k:?}")).take(12).collect())
            .unwrap_or_default();
        let text = ud.to_string().unwrap_or_default();
        Err(LuaError::runtime(format!("keyframe: a userdata of a type keyframes don't know (metatable keys {keys:?}; tostring {text:?})")))
    }

    fn load(self, lua: &Lua, payload: &[u8]) -> LuaResult<LuaAnyUserData> {
        use crate::engine::api::*;
        let mut r = payload;
        let id = |r: &mut &[u8]| -> LuaResult<u32> { Ok(get_var(r)? as u32) };
        Ok(match self {
            UserdataKind::Army => lua.create_userdata(army::ArmyHandle(id(&mut r)?))?,
            UserdataKind::Command => lua.create_userdata(command::CommandHandle(id(&mut r)?))?,
            UserdataKind::Decal => lua.create_userdata(presentation::DecalHandle(id(&mut r)?))?,
            UserdataKind::EconomyEvent => lua.create_userdata(economy_event::EconomyEventHandle(id(&mut r)?))?,
            UserdataKind::Effect => lua.create_userdata(effect::EffectHandle(id(&mut r)?))?,
            UserdataKind::Entity => lua.create_userdata(entity::EntityHandle(id(&mut r)?))?,
            UserdataKind::Manipulator => lua.create_userdata(manipulator::ManipulatorHandle(id(&mut r)?))?,
            UserdataKind::Personality => lua.create_userdata(personality::PersonalityHandle(id(&mut r)?))?,
            UserdataKind::Platoon => lua.create_userdata(platoon::PlatoonHandle(id(&mut r)?))?,
            UserdataKind::ScriptTask => lua.create_userdata(script_task::ScriptTaskHandle(id(&mut r)?))?,
            UserdataKind::Category => {
                let n = get_var(&mut r)? as usize;
                let mut words = Vec::with_capacity(n);
                for _ in 0..n {
                    words.push(u64::from_le_bytes(take(&mut r, 8)?.try_into().unwrap()));
                }
                lua.create_userdata(categories::EntityCategory(crate::engine::sim::category::CategorySet::from_words(words)))?
            }
            UserdataKind::Sound => {
                let bank = get_str(&mut r)?;
                let cue = get_str(&mut r)?;
                let lod_cutoff = if take(&mut r, 1)?[0] == 1 { Some(get_str(&mut r)?.into()) } else { None };
                lua.create_userdata(sound::SoundHandle { bank: bank.into(), cue: cue.into(), lod_cutoff })?
            }
            UserdataKind::Weapon => {
                let unit = id(&mut r)?;
                let index = id(&mut r)?;
                lua.create_userdata(weapon::WeaponHandle { unit, index })?
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Bytes

fn put_var(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_var(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn take<'a>(r: &mut &'a [u8], n: usize) -> LuaResult<&'a [u8]> {
    if r.len() < n {
        return Err(LuaError::runtime("keyframe: truncated"));
    }
    let (head, rest) = r.split_at(n);
    *r = rest;
    Ok(head)
}

fn get_var(r: &mut &[u8]) -> LuaResult<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = take(r, 1)?[0];
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(LuaError::runtime("keyframe: bad varint"))
}

fn get_str(r: &mut &[u8]) -> LuaResult<String> {
    let n = get_var(r)? as usize;
    String::from_utf8(take(r, n)?.to_vec()).map_err(|_| LuaError::runtime("keyframe: bad string"))
}
