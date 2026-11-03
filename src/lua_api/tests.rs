//! The typed Lua API's macros and derives on a bare VM.

use super::*;
use crate::engine::api::moho;
use crate::engine::sim::math::Fx;

/// A layer, for the tests.
#[derive(Clone, Copy, Debug, PartialEq, LuaEnum)]
#[lua(name = "TestLayer")]
enum TestLayer {
    Land,
    #[lua(rename = "Seabed")]
    Sea,
}

/// Air movement parameters, for the tests.
#[derive(Debug, PartialEq, LuaTable)]
#[lua(rename_all = "PascalCase")]
struct TestAir {
    /// Turn multiplier while circling.
    #[lua(default = "Fx::from_num(1)")]
    circling_turn_mult: Fx,
    #[lua(rename = "MaxAirspeed")]
    max_speed: Option<Fx>,
    winged: bool,
    #[lua(default)]
    name: String,
    #[lua(skip)]
    cache: u32,
    layer: Option<TestLayer>,
}

#[test]
fn lua_enum_round_trips_and_names_bad_values() {
    let lua = Lua::new();
    let v: TestLayer = lua.load("return 'Seabed'").eval().unwrap();
    assert_eq!(v, TestLayer::Sea);
    assert_eq!(TestLayer::Land.lua_str(), "Land");
    let s: String = lua.load("return ...").call(TestLayer::Sea).unwrap();
    assert_eq!(s, "Seabed");
    let err = lua.load("return 'Sea'").eval::<TestLayer>().unwrap_err().to_string();
    assert!(err.contains("'Sea' is not one of Land|Seabed"), "{err}");
    assert_eq!(TestLayer::LUA_VARIANTS, &["Land", "Seabed"]);
}

#[test]
fn lua_table_reads_defaults_fx_and_reports_missing_and_unknown() {
    let lua = Lua::new();
    let t: LuaTable = lua.load("return { MaxAirspeed = 12.5, Winged = true, Layer = 'Land', BankFacter = 1 }").eval().unwrap();
    let air: TestAir = lua.unpack(LuaValue::Table(t.clone())).unwrap();
    assert_eq!(
        air,
        TestAir { circling_turn_mult: Fx::from_num(1), max_speed: Some(crate::engine::sim::math::from_lua(12.5)), winged: true, name: String::new(), cache: 0, layer: Some(TestLayer::Land) }
    );
    assert_eq!(TestAir::unknown_keys(&t).unwrap(), vec!["BankFacter".to_string()]);
    // A required key missing names the struct and key.
    let err = lua.load("return {}").eval::<TestAir>().unwrap_err().to_string();
    assert!(err.contains("TestAir.Winged: missing"), "{err}");
    // A wrong type too.
    let err = lua.load("return { Winged = true, MaxAirspeed = 'fast' }").eval::<TestAir>().unwrap_err().to_string();
    assert!(err.contains("TestAir.MaxAirspeed"), "{err}");
}

#[test]
fn lua_table_writes_in_place() {
    let lua = Lua::new();
    let air = TestAir { circling_turn_mult: Fx::from_num(2), max_speed: None, winged: true, name: "a".into(), cache: 7, layer: Some(TestLayer::Sea) };
    let t: LuaTable = lua.load("return { MaxAirspeed = 3, Other = 1 }").eval().unwrap();
    air.write_into(&lua, &t).unwrap();
    let (turn, max, winged, name, layer, other): (f64, Option<f64>, bool, String, String, i64) =
        lua.load("local t = ... return t.CirclingTurnMult, t.MaxAirspeed, t.Winged, t.Name, t.Layer, t.Other").call(t).unwrap();
    assert_eq!((turn, max, winged, name.as_str(), layer.as_str(), other), (2.0, None, true, "a", "Seabed", 1));
}

fn registry_entry(lua: &Lua, name: &str) -> FnEntry {
    lua.app_data_ref::<ApiRegistry>().unwrap().named(name).first().cloned().cloned().unwrap()
}

#[test]
fn lua_fn_registers_and_records_names_docs_status() {
    let lua = Lua::new();
    lua_fn!(&lua, global,
        /// Adds two numbers.
        Add = |_, (a, b): (f64, Option<f64>)| Ok(a + b.unwrap_or(0.0)))
    .unwrap();
    lua_fn!(&lua, global,
        #[lua(stub)]
        Later = |_, _: LuaMultiValue| Ok(()))
    .unwrap();
    assert_eq!(lua.load("return Add(1, 2)").eval::<f64>().unwrap(), 3.0);
    let f = registry_entry(&lua, "Add");
    assert_eq!(f.params, Some(&[Some("a"), Some("b")][..]));
    assert_eq!(f.doc, "Adds two numbers.");
    let sig = emit::signature(&f);
    assert_eq!(sig.params, vec![("a".to_string(), LuaTy::Number), ("b".to_string(), LuaTy::Optional(Box::new(LuaTy::Number)))]);
    assert_eq!(sig.rets, vec![LuaTy::Number]);
    assert_eq!(registry_entry(&lua, "Later").status, Status::Stub);
    let reg = lua.app_data_ref::<ApiRegistry>().unwrap();
    let luau = emit::luau(&reg, "test");
    assert!(luau.contains("--- Adds two numbers.\ndeclare function Add(a: number, b: number?): number"), "{luau}");
    let emmy = emit::emmy(&reg, "test");
    assert!(emmy.contains("---@param a number\n---@param b number?\n---@return number\nfunction Add(a, b) end"), "{emmy}");
}

#[test]
fn lua_method_types_self_and_drops_it_from_the_signature() {
    let lua = Lua::new();
    moho::register_classes(&lua, &[("entity_methods", &[]), ("unit_methods", &["entity_methods"])]).unwrap();
    lua_method!(&lua, moho::Unit,
        /// Sets the build rate.
        SetBuildRate = |_, (this, rate): (Obj<moho::Unit>, f64)| this.raw_set("rate", rate))
    .unwrap();
    lua.load("local u = setmetatable({}, { __index = moho.unit_methods }) u:SetBuildRate(4) assert(u.rate == 4)").exec().unwrap();
    let f = registry_entry(&lua, "SetBuildRate");
    let sig = emit::signature(&f);
    assert_eq!(sig.params, vec![("rate".to_string(), LuaTy::Number)]);
    let reg = lua.app_data_ref::<ApiRegistry>().unwrap();
    let luau = emit::luau(&reg, "test");
    assert!(luau.contains("declare extern type Unit extends Entity with\n    --- Sets the build rate.\n    function SetBuildRate(self, rate: number): ()\nend"), "{luau}");
    let emmy = emit::emmy(&reg, "test");
    assert!(emmy.contains("---@class moho.unit_methods : moho.entity_methods"), "{emmy}");
    assert!(emmy.contains("function unit_methods:SetBuildRate(rate) end"), "{emmy}");
}

#[test]
fn bad_argument_is_explained_with_the_signature() {
    let lua = Lua::new();
    moho::register_classes(&lua, &[("unit_methods", &[])]).unwrap();
    lua_method!(&lua, moho::Unit, SetBuildRate = |_, (_this, _rate): (Obj<moho::Unit>, f64)| Ok(())).unwrap();
    lua_fn!(&lua, global, Spawn = |_, (_bp, _army): (String, i64)| Ok(())).unwrap();
    let method = lua.load("local u = setmetatable({}, { __index = moho.unit_methods }) u:SetBuildRate(nil)").exec().unwrap_err();
    let text = errors::Explained(&lua, &method).to_string();
    assert!(text.contains("in Unit:SetBuildRate(rate: number): argument #2 `rate` should be number"), "{text}");
    let dot = lua.load("local u = setmetatable({}, { __index = moho.unit_methods }) u.SetBuildRate(5)").exec().unwrap_err();
    let text = errors::Explained(&lua, &dot).to_string();
    assert!(text.contains("argument #1 is `self` (call it with `:`)"), "{text}");
    let global = lua.load("Spawn('uel0101', 'one')").exec().unwrap_err();
    let text = errors::Explained(&lua, &global).to_string();
    assert!(text.contains("in Spawn(bp: string, army: number): argument #2 `army` should be number"), "{text}");
    // Tracebacks and debug.info name native functions.
    assert!(method.to_string().contains("in function 'Unit:SetBuildRate'"), "{method}");
    let name: String = lua.load("return debug.info(moho.unit_methods.SetBuildRate, 'n')").eval().unwrap();
    assert_eq!(name, "Unit:SetBuildRate");
}

callbacks! {
    /// Test callbacks (on a class no real block uses).
    moho::CDamage {
        /// After creation.
        OnCreate();
        OnStopBeingBuilt(builder: Option<LuaTable>, layer: TestLayer);
    }
}

#[test]
fn callbacks_dispatch_typed_and_are_recorded() {
    let lua = Lua::new();
    moho::register_classes(&lua, &[("CDamage", &[])]).unwrap();
    let obj: LuaTable = lua.load("return { OnStopBeingBuilt = function(self, b, layer) self.got = layer end }").eval().unwrap();
    CDamageCallbacks::on_stop_being_built(&lua, &obj, "test", None, TestLayer::Sea).unwrap();
    assert_eq!(obj.get::<String>("got").unwrap(), "Seabed");
    // Missing callback: a no-op.
    CDamageCallbacks::on_create(&lua, &obj, "test").unwrap();
    let reg = lua.app_data_ref::<ApiRegistry>().unwrap();
    let luau = emit::luau(&reg, "test");
    assert!(luau.contains("type CDamageCallbacks = {\n    --- After creation.\n    OnCreate: (self: CDamage) -> (),\n    OnStopBeingBuilt: (self: CDamage, builder: { [any]: any }?, layer: TestLayer) -> (),\n}"), "{luau}");
    assert!(luau.contains("type TestLayer = \"Land\" | \"Seabed\""), "{luau}");
}

#[test]
fn registry_keeps_the_last_registration() {
    let lua = Lua::new();
    crate::engine::api::pending::register(&lua).unwrap();
    let name = crate::engine::api::pending::SIM[0];
    assert_eq!(registry_entry(&lua, name).status, Status::Stub);
    let g = lua.globals();
    register_fn(&lua, &g, "", name, None, "", Status::Implemented, false, |_, ()| Ok(())).unwrap();
    assert_eq!(registry_entry(&lua, name).status, Status::Implemented);
}

#[test]
fn compare_flags_missing_extra_typos_and_arity() {
    let lua = Lua::new();
    moho::register_classes(&lua, &[("unit_methods", &[])]).unwrap();
    lua_method!(&lua, moho::Unit, GetBuildRat = |_, _this: Obj<moho::Unit>| Ok(1.0)).unwrap();
    lua_method!(&lua, moho::Unit, SetPaused = |_, (_this, _on): (Obj<moho::Unit>, bool)| Ok(())).unwrap();
    let mut faf = compare::FafApi::default();
    for (name, params) in [("GetBuildRate", vec![]), ("SetPaused", vec!["paused".to_string(), "extra".to_string()]), ("Kill", vec![])] {
        faf.fns.insert((Scope::Class("unit_methods".into()), name.to_string()), compare::FafFn { params, file: "Unit.lua".into(), line: 1 });
    }
    let reg = lua.app_data_ref::<ApiRegistry>().unwrap();
    let (md, s) = compare::report(&reg, &faf, "test");
    assert_eq!((s.missing, s.not_in_faf, s.likely_typos, s.arity), (2, 1, 1, 1), "{md}");
    assert!(md.contains("`unit_methods:GetBuildRat`: FAF has `GetBuildRate`"), "{md}");
}
