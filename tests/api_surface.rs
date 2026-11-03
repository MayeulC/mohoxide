//! The Lua-visible engine API, as both VMs see it straight after the engine
//! registers it (before any FAF script runs): every global function, every
//! function in a global table, and every method of every `moho` class, each
//! marked `native` (Rust) or `luau` (an engine Luau shim).
//!
//! The list is read from the live tables, not from `lua_api`'s registry, so
//! it independently catches a registration that was renamed, dropped or
//! added. Compared with `tests/golden/api_surface.txt`; when a change is
//! intended: `UPDATE_GOLDEN=1 cargo nextest run --test api_surface` and
//! review the diff.

use std::sync::{Arc, RwLock};

use mlua::prelude::*;
use rts_core_engine::engine::api;
use rts_core_engine::engine::lua::script_cache::ScriptCache;
use rts_core_engine::engine::vfs::Vfs;
use rts_core_engine::ui::UserVm;

const GOLDEN: &str = "tests/golden/api_surface.txt";

/// Walks `_G` (and one level into `moho`'s classes) and lists the functions.
const WALK: &str = r#"
    local out = {}
    local function kind(f) return debug.info(f, "s") == "[C]" and "native" or "luau" end
    local function walk(prefix, t, depth)
        for k, v in pairs(t) do
            if type(k) == "string" then
                local name = prefix .. k
                if type(v) == "function" then
                    table.insert(out, name .. " " .. kind(v))
                elseif type(v) == "table" and depth > 0 and v ~= _G then
                    walk(name .. ".", v, depth - 1)
                end
            end
        end
    end
    walk("", _G, 1)
    walk("moho.", rawget(_G, "moho") or {}, 1)
    table.sort(out)
    return out
"#;

fn surface(lua: &Lua) -> Vec<String> {
    lua.load(WALK).set_name("=api_surface").eval().expect("walk")
}

fn empty_vfs() -> Arc<RwLock<Vfs>> {
    Arc::new(RwLock::new(Vfs::new(Vec::new(), Vec::new()).expect("vfs")))
}

/// The sim VM as `Engine::new` builds it, without the game's files.
fn sim_vm() -> Lua {
    let vfs = empty_vfs();
    let lua = Lua::new();
    lua.set_app_data(vfs.clone());
    lua.set_app_data(Arc::new(ScriptCache::new(vfs)));
    api::register_safe(&lua).expect("sim API");
    lua
}

#[test]
fn lua_visible_api_is_unchanged() {
    let sim = sim_vm();
    let user = UserVm::new(empty_vfs()).expect("user vm");
    let mut lines = Vec::new();
    lines.extend(surface(&sim).into_iter().map(|l| format!("sim  {l}")));
    lines.extend(surface(user.lua()).into_iter().map(|l| format!("user {l}")));
    let text = lines.join("\n") + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &text).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(GOLDEN).expect("golden file (UPDATE_GOLDEN=1 to create)");
    if golden != text {
        let old: std::collections::BTreeSet<&str> = golden.lines().collect();
        let new: std::collections::BTreeSet<&str> = text.lines().collect();
        let gone: Vec<_> = old.difference(&new).collect();
        let added: Vec<_> = new.difference(&old).collect();
        panic!("Lua-visible API changed.\nremoved: {gone:#?}\nadded: {added:#?}\n(UPDATE_GOLDEN=1 if intended)");
    }
}

/// Native functions registered without `lua_api` (`lua_fn!`, `lua_method!`,
/// `moho::add_method`): they have no debug name, where Luau's built-ins and
/// everything `lua_api` registers do. Such a function is missing from the
/// generated types and its errors aren't explained. Lists them all.
const UNNAMED: &str = r#"
    local out = {}
    local function walk(prefix, t, depth)
        for k, v in pairs(t) do
            if type(k) == "string" then
                local name = prefix .. k
                if type(v) == "function" then
                    local n = debug.info(v, "n")
                    if debug.info(v, "s") == "[C]" and (n == nil or n == "") then table.insert(out, name) end
                elseif type(v) == "table" and depth > 0 and v ~= _G then
                    walk(name .. ".", v, depth - 1)
                end
            end
        end
    end
    walk("", _G, 1)
    walk("moho.", rawget(_G, "moho") or {}, 1)
    table.sort(out)
    return out
"#;

/// Native functions that aren't the engine's: mlua installs its own
/// `collectgarbage` for Luau, unnamed.
const NOT_OURS: &[&str] = &["collectgarbage"];

#[test]
fn every_native_function_goes_through_lua_api() {
    let sim = sim_vm();
    let user = UserVm::new(empty_vfs()).expect("user vm");
    let mut missing: Vec<String> = Vec::new();
    for (vm, lua) in [("sim", &sim), ("user", user.lua())] {
        let names: Vec<String> = lua.load(UNNAMED).set_name("=unnamed").eval().expect("walk");
        missing.extend(names.into_iter().filter(|n| !NOT_OURS.contains(&n.as_str())).map(|n| format!("{vm} {n}")));
    }
    assert!(missing.is_empty(), "{} native functions registered without lua_api:\n{}", missing.len(), missing.join("\n"));
}
