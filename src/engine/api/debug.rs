//! `debug.getinfo` — Lua 5.0 style debug info on top of Luau.
//!
//! Luau only ships `debug.info`, but FAF scripts use the Lua 5.0 table-returning
//! `debug.getinfo(level|func [, what])`. Implemented natively so FAF's profiler
//! and warning helpers don't pay for a Lua shim on every call.
//!
//! Only the fields FAF reads are populated: `source`, `short_src`, `what`,
//! `linedefined`, `currentline`, `name`, `nups`, `func` (function form only).
//! `source` uses the Lua convention: `@<path>` for files and `=[C]` for natives,
//! which FAF compares against (see `/lua/system/Blueprints.lua` GetSource).
//!
//! Also FAF's memory estimates: `debug.allocatedsize` (a Luau shim) and
//! `debug.allocatedrsize` (native, swapped in by the engine hook of
//! `/lua/system/debug.lua`).

use std::ffi::{c_int, c_void};

use mlua::ffi;
use mlua::prelude::*;

use crate::lua_api::lua_fn;

const DEFAULT_WHAT: &str = "flnSu";

pub fn register(lua: &Lua) -> LuaResult<()> {
    let debug: LuaTable = lua.globals().get("debug")?;
    lua_fn!(lua, table(debug, "debug"),
        /// Lua 5.0 style `debug.getinfo(level|func [, what])`: a table with
        /// `source`, `short_src`, `what`, `linedefined`, `currentline`, `name`,
        /// `nups`, `func` (function form only), as selected by `what`.
        getinfo = |lua, args: LuaMultiValue| getinfo(lua, args))?;
    // A Luau shim: FAF's `debug.allocatedrsize` calls it on every table
    // and string of a structure (the nav mesh report after generation: 370 ms
    // natively, each entry crossing into Rust as a ref-counted value).
    lua.load(ALLOCATED_SIZE).set_name("=[engine debug.allocatedsize]").call::<()>(debug.clone())?;
    // FAF's `/lua/system/debug.lua` defines `debug.allocatedrsize` in Lua;
    // the engine hook of that file swaps this in.
    // SAFETY: `allocated_recursive_size` only uses the stack slots it sets up.
    let recursive = unsafe { lua.create_c_function(allocated_recursive_size)? };
    crate::lua_api::name_function(lua, &recursive, "debug.allocatedrsize")?;
    debug.raw_set("__allocatedrsize", recursive)
}

/// FAF binary-patch `debug.allocatedsize`: approximate bytes an object uses in
/// FA's 32-bit Lua 5.0 (header + 8-byte array slots + 20-byte hash nodes, hash
/// part rounded to a power of two; strings 16 + length + 1). Only used for
/// diagnostics (`repr`, memory reports), so an estimate is fine; it is
/// deterministic (depends only on contents).
const ALLOCATED_SIZE: &str = r#"
local debug = ...
local type, next, rawlen = type, next, rawlen
debug.allocatedsize = function(v)
    local t = type(v)
    if t == 'table' then
        local array = rawlen(v)
        local total = 0
        for _ in next, v do
            total += 1
        end
        local hash = total - array
        local slots = 0
        if hash > 0 then
            slots = 1
            while slots < hash do
                slots *= 2
            end
        end
        return 32 + array * 8 + slots * 20
    elseif t == 'string' then
        return 16 + #v + 1
    end
    return 0
end
"#;

/// The stack slots of [`allocated_recursive_size`].
const ELEMENT: c_int = 1;
const IGNORE: c_int = 2;
/// The tables found but not walked yet, `[1..pending]`.
const PENDING: c_int = 3;
/// The table being walked, then its key and value under `lua_next`.
const TABLE: c_int = 4;
const VALUE: c_int = 6;

/// FAF's `debug.allocatedrsize(element [, ignore])`: the bytes `element` and
/// everything reachable through its tables' values use, by
/// `debug.allocatedsize`'s estimate. Each table and string counts once; any
/// other value 8 bytes per reference; a key in `ignore` is skipped in every
/// table. FAF sums its nav mesh with it after generating it (~0.6 s in
/// Lua), so it walks the raw stack: no Lua loop, and no ref-counted mlua
/// value per entry. The total doesn't depend on the walk's order.
unsafe extern "C-unwind" fn allocated_recursive_size(state: *mut ffi::lua_State) -> c_int {
    unsafe {
        if ffi::lua_type(state, ELEMENT) == ffi::LUA_TNIL {
            ffi::lua_pushnumber(state, 0.0);
            return 1;
        }
        let has_ignore = ffi::lua_type(state, IGNORE) == ffi::LUA_TTABLE;
        ffi::lua_settop(state, IGNORE);
        ffi::lua_createtable(state, 0, 0);
        let mut walk = Walk::default();
        walk.visit(state, ELEMENT);
        while walk.pending > 0 {
            ffi::lua_rawgeti(state, PENDING, walk.pending);
            walk.pending -= 1;
            let array = ffi::lua_objlen(state, TABLE);
            let mut entries = 0;
            ffi::lua_pushnil(state);
            while ffi::lua_next(state, TABLE) != 0 {
                entries += 1;
                if !(has_ignore && is_ignored(state)) {
                    walk.visit(state, VALUE);
                }
                ffi::lua_pop(state, 1);
            }
            ffi::lua_pop(state, 1);
            walk.bytes += table_size(array, entries);
        }
        ffi::lua_pushnumber(state, walk.bytes as f64);
        1
    }
}

#[derive(Default)]
struct Walk {
    /// The tables and strings counted. Only asked, never iterated.
    #[allow(clippy::disallowed_types)]
    seen: std::collections::HashSet<*const c_void>,
    pending: ffi::lua_Integer,
    bytes: u64,
}

impl Walk {
    /// Counts the value at `index`; a table not seen yet is queued to walk.
    unsafe fn visit(&mut self, state: *mut ffi::lua_State, index: c_int) {
        unsafe {
            match ffi::lua_type(state, index) {
                ffi::LUA_TTABLE => {
                    if self.seen.insert(ffi::lua_topointer(state, index)) {
                        self.pending += 1;
                        ffi::lua_pushvalue(state, index);
                        ffi::lua_rawseti(state, PENDING, self.pending);
                    }
                }
                // Luau interns every string: one pointer per contents.
                ffi::LUA_TSTRING => {
                    if self.seen.insert(ffi::lua_topointer(state, index)) {
                        self.bytes += 16 + ffi::lua_objlen(state, index) as u64 + 1;
                    }
                }
                _ => self.bytes += 8,
            }
        }
    }
}

/// `ignore[key]` is truthy, for the key under `lua_next`.
unsafe fn is_ignored(state: *mut ffi::lua_State) -> bool {
    unsafe {
        ffi::lua_pushvalue(state, VALUE - 1);
        ffi::lua_rawget(state, IGNORE);
        let ignored = ffi::lua_toboolean(state, -1) != 0;
        ffi::lua_pop(state, 1);
        ignored
    }
}

/// `debug.allocatedsize` of a table with `array` slots (its `rawlen`) and
/// `entries` in all.
fn table_size(array: usize, entries: usize) -> u64 {
    let hash = entries.saturating_sub(array);
    let slots = if hash > 0 { hash.next_power_of_two() } else { 0 };
    32 + array as u64 * 8 + slots as u64 * 20
}

fn getinfo(lua: &Lua, args: LuaMultiValue) -> LuaResult<LuaValue> {
    let mut args = args.into_iter();
    let target = args.next().unwrap_or(LuaNil);
    let what = match args.next() {
        Some(LuaValue::String(s)) => s.to_str()?.to_owned(),
        _ => DEFAULT_WHAT.to_owned(),
    };

    match target {
        LuaValue::Function(f) => function_info(lua, f, &what).map(LuaValue::Table),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let level = match target {
                LuaValue::Integer(i) => i as i64,
                LuaValue::Number(n) => n as i64,
                _ => unreachable!(),
            };
            if level < 0 {
                return Ok(LuaNil);
            }
            // Level 0 is this native function, 1 its caller — same as Lua 5.0.
            match stack_info(lua, level as usize, &what)? {
                Some(t) => Ok(LuaValue::Table(t)),
                None => Ok(LuaNil),
            }
        }
        // Thread form (`getinfo(co, level, what)`) is unused by FAF.
        _ => Ok(LuaNil),
    }
}

fn normalise_source(source: Option<&str>, what: &str) -> String {
    match source {
        Some(s) if s.starts_with('@') || s.starts_with('=') => s.to_owned(),
        Some(s) if what != "C" => format!("@{s}"),
        _ => "=[C]".to_owned(),
    }
}

fn stack_info(lua: &Lua, level: usize, what: &str) -> LuaResult<Option<LuaTable>> {
    let Some(dbg) = lua.inspect_stack(level) else {
        return Ok(None);
    };
    let t = lua.create_table_with_capacity(0, 8)?;
    for c in what.chars() {
        match c {
            'S' => {
                let src = dbg.source();
                t.set("source", normalise_source(src.source.as_deref(), src.what))?;
                t.set("short_src", src.short_src.as_deref())?;
                t.set("what", if src.what == "main" { "main" } else { src.what })?;
                t.set("linedefined", src.line_defined.map(|l| l as i64).unwrap_or(-1))?;
            }
            'l' => t.set("currentline", dbg.curr_line())?,
            'n' => {
                if let Some(name) = dbg.names().name {
                    t.set("name", name.as_ref())?;
                }
            }
            'u' => t.set("nups", dbg.stack().num_ups)?,
            _ => {}
        }
    }
    Ok(Some(t))
}

fn function_info(lua: &Lua, f: LuaFunction, what: &str) -> LuaResult<LuaTable> {
    let info = f.info();
    let t = lua.create_table_with_capacity(0, 8)?;
    for c in what.chars() {
        match c {
            'S' => {
                t.set("source", normalise_source(info.source.as_deref(), info.what))?;
                t.set("short_src", info.short_src.as_deref())?;
                t.set("what", info.what)?;
                t.set("linedefined", info.line_defined.map(|l| l as i64).unwrap_or(-1))?;
            }
            'l' => t.set("currentline", -1)?,
            'n' => {
                if let Some(name) = &info.name {
                    t.set("name", name.as_str())?;
                }
            }
            'f' => t.set("func", f.clone())?,
            _ => {}
        }
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua() -> Lua {
        let lua = Lua::new();
        register(&lua).unwrap();
        lua
    }

    #[test]
    fn level_one_is_caller_chunk() {
        let lua = lua();
        let (source, line): (String, i64) = lua
            .load("local i = debug.getinfo(1)\nreturn i.source, i.currentline")
            .set_name("/lua/system/Blueprints.lua")
            .eval()
            .unwrap();
        assert_eq!(source, "@/lua/system/Blueprints.lua");
        assert_eq!(line, 1);
    }

    #[test]
    fn native_frames_are_c() {
        let lua = lua();
        let source: String = lua
            .load("return debug.getinfo(print, 'S').source")
            .eval()
            .unwrap();
        assert_eq!(source, "=[C]");
    }

    #[test]
    fn out_of_range_level_is_nil() {
        let lua = lua();
        let v: LuaValue = lua.load("return debug.getinfo(100)").eval().unwrap();
        assert!(v.is_nil());
    }

    #[test]
    fn allocatedsize_counts_array_and_hash() {
        let lua = lua();
        let n: i64 = lua
            .load("return debug.allocatedsize({1, 2, 3, a = 1, b = 2, c = 3})")
            .eval()
            .unwrap();
        assert_eq!(n, 32 + 3 * 8 + 4 * 20);
    }

    /// FAF's `debug.allocatedrsize` (`/lua/system/debug.lua`), verbatim.
    const FAF_ALLOCATED_RECURSIVE_SIZE: &str = r#"
local debug = ...
local type = type
return function(element, ignore)
    ignore = ignore or { }

    -- has no allocated bytes
    if element == nil then
        return 0
    end

    -- applies to tables and strings, to prevent counting them multiple times
    local seen = {}

    -- prepare stack to prevent recursion
    local allocatedSize = 0
    local stack = { element }
    local head = 2

    while head > 1 do

        head = head - 1
        local value = stack[head]
        stack[head] = nil

        local size = debug.allocatedsize(value)

        -- size of usual value
        if size == 0 then
            allocatedSize = allocatedSize + 8

            -- size of string
        elseif type(value) ~= 'table' then
            if not seen[value] then
                seen[value] = true
                allocatedSize = allocatedSize + size
            end

            -- size of table
        else
            if not seen[value] then
                allocatedSize = allocatedSize + size
                seen[value] = true
                for k, v in value do
                    if not ignore[k] then
                        stack[head] = v
                        head = head + 1
                    end
                end
            end
        end
    end

    return allocatedSize
end
"#;

    #[test]
    fn allocatedrsize_matches_faf() {
        let lua = lua();
        let debug: LuaTable = lua.globals().get("debug").unwrap();
        let faf: LuaFunction = lua.load(FAF_ALLOCATED_RECURSIVE_SIZE).call(debug.clone()).unwrap();
        let native: LuaFunction = debug.get("__allocatedrsize").unwrap();
        let cases: LuaTable = lua
            .load(
                r#"
                local shared = { 'shared', x = 1 }
                local cyclic = { name = 'loop' }
                cyclic.self = cyclic
                cyclic.list = { cyclic, cyclic, 'loop' }
                -- A nav mesh in miniature: leaves pointing at each other, with
                -- labels whose `Node` the report ignores.
                local leaves, labels = {}, {}
                for i = 1, 500 do
                    leaves[i] = { Label = i % 7, Size = i * 0.5, Neighbors = {}, Name = 'leaf' .. (i % 13) }
                end
                for i = 1, 500 do
                    local n = leaves[i].Neighbors
                    for j = 1, i % 5 do n[j] = leaves[(i * 31 + j) % 500 + 1] end
                    n.Corner = leaves[(i * 17) % 500 + 1]
                end
                for i = 1, 7 do labels[i] = { Area = i, Node = leaves[i], Extractors = {} } end
                return {
                    { 1 }, { 'a string' }, { true }, { print }, { {} },
                    { { 1, 2, 3, a = 1, b = 'b', c = { 'b' } } },
                    { { shared, shared, { shared } } },
                    { cyclic },
                    { { [{}] = 'table key', [print] = print, [1.5] = false } },
                    { leaves }, { labels }, { labels, { Node = true } },
                    { leaves, { Neighbors = true, Label = true } },
                }
                "#,
            )
            .eval()
            .unwrap();
        for (i, case) in cases.sequence_values::<LuaTable>().enumerate() {
            let args = case.unwrap().sequence_values::<LuaValue>().collect::<LuaResult<Vec<_>>>().unwrap();
            let args = LuaMultiValue::from_iter(args);
            let expected: f64 = faf.call(args.clone()).unwrap();
            let got: f64 = native.call(args).unwrap();
            assert_eq!(got, expected, "case {}", i + 1);
        }
        let nil: f64 = native.call(LuaNil).unwrap();
        assert_eq!(nil, 0.0);
    }

    #[test]
    fn function_form_returns_func() {
        let lua = lua();
        let same: bool = lua
            .load("local function f() end\nreturn debug.getinfo(f).func == f")
            .eval()
            .unwrap();
        assert!(same);
    }
}
