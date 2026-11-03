//! Moho classes as Rust types: a marker per class (`moho::Unit` is
//! `moho.unit_methods`) and `Obj<C>`, a script object known to be of class
//! `C`, for typed `self` parameters.

use std::marker::PhantomData;
use std::ops::Deref;

use mlua::prelude::*;

use super::ty::{LuaTy, LuaType};

/// A moho class (`moho.<CLASS>` in Lua).
pub trait MohoClass {
    /// The moho class table's name (`unit_methods`).
    const CLASS: &'static str;
    /// The class's name in the generated Luau definitions (`Unit`).
    const LUA_NAME: &'static str;
}

/// Classes whose objects are sim entities (carry an `EntityHandle`).
pub trait EntityClass: MohoClass {}

macro_rules! moho_classes {
    ($( $(#[$m:meta])* $marker:ident = $class:literal $(, entity = $entity:tt)? ;)+) => {
        $(
            $(#[$m])*
            #[doc = concat!("`moho.", $class, "`.")]
            pub struct $marker;
            impl MohoClass for $marker {
                const CLASS: &'static str = $class;
                const LUA_NAME: &'static str = stringify!($marker);
            }
            $(moho_classes!(@entity $marker $entity);)?
        )+

        /// Every class: (moho name, Luau name).
        pub const ALL: &[(&str, &str)] = &[$(($class, stringify!($marker))),+];
    };
    (@entity $marker:ident true) => { impl EntityClass for $marker {} };
}

moho_classes! {
    // Sim (FAF's /engine/Sim/*.lua).
    Entity = "entity_methods", entity = true;
    Manipulator = "manipulator_methods";
    Unit = "unit_methods", entity = true;
    Projectile = "projectile_methods", entity = true;
    Prop = "prop_methods", entity = true;
    Blip = "blip_methods", entity = true;
    Shield = "shield_methods", entity = true;
    CollisionBeamEntity = "CollisionBeamEntity", entity = true;
    Weapon = "weapon_methods";
    AIBrain = "aibrain_methods";
    AIPersonality = "aipersonality_methods";
    CAiAttackerImpl = "CAiAttackerImpl_methods";
    Navigator = "navigator_methods";
    Platoon = "platoon_methods";
    ScriptTask = "ScriptTask_Methods";
    AimManipulator = "AimManipulator";
    AnimationManipulator = "AnimationManipulator";
    BoneEntityManipulator = "BoneEntityManipulator";
    BuilderArmManipulator = "BuilderArmManipulator";
    CollisionManipulator = "CollisionManipulator";
    FootPlantManipulator = "FootPlantManipulator";
    RotateManipulator = "RotateManipulator";
    SlaveManipulator = "SlaveManipulator";
    SlideManipulator = "SlideManipulator";
    StorageManipulator = "StorageManipulator";
    ThrustManipulator = "ThrustManipulator";
    CDamage = "CDamage";
    CDecalHandle = "CDecalHandle";
    EconomyEvent = "EconomyEvent";
    IEffect = "IEffect";
    MotorFallDown = "MotorFallDown";
    // Core (both VMs).
    Sound = "sound_methods";
    EntityCategory = "EntityCategory";
    CPrefetchSet = "CPrefetchSet";
    // User (FAF's /engine/User/*.lua).
    Control = "control_methods";
    Group = "group_methods";
    Bitmap = "bitmap_methods";
    Text = "text_methods";
    Frame = "frame_methods";
    Border = "border_methods";
    Edit = "edit_methods";
    ItemList = "item_list_methods";
    Histogram = "histogram_methods";
    Scrollbar = "scrollbar_methods";
    Dragger = "dragger_methods";
    Movie = "movie_methods";
    Mesh = "mesh_methods";
    Cursor = "cursor_methods";
    DiscoveryService = "discovery_service_methods";
    Lobby = "lobby_methods";
    UIWorldView = "UIWorldView";
    WorldMesh = "world_mesh_methods";
    WldUIProvider = "WldUIProvider_methods";
    UserUnit = "userunit_methods";
    UIMapPreview = "ui_map_preview_methods";
    PathDebugger = "PathDebugger_methods";
    UserDecal = "userDecal_methods";
    CameraImpl = "CameraImpl_methods";
}

/// The Luau name of moho class `class` (`unit_methods` -> `Unit`).
pub fn luau_name(class: &str) -> Option<&'static str> {
    ALL.iter().find(|(c, _)| *c == class).map(|(_, n)| *n)
}

/// A script object of moho class `C` (`self` of a native method, or an
/// argument). It derefs to the object's table, so code written against
/// `&LuaTable` keeps working; converting from Lua only checks that the value
/// is a table (the native handle is checked where it's read, as before).
pub struct Obj<C> {
    table: LuaTable,
    _class: PhantomData<fn() -> C>,
}

impl<C> Obj<C> {
    /// Wraps a table already known to be an object of class `C`.
    pub fn new(table: LuaTable) -> Self {
        Self { table, _class: PhantomData }
    }

    pub fn table(&self) -> &LuaTable {
        &self.table
    }

    pub fn into_table(self) -> LuaTable {
        self.table
    }
}

impl<C: EntityClass> Obj<C> {
    /// The entity behind the object.
    pub fn id(&self) -> LuaResult<crate::engine::sim::entity::EntityId> {
        crate::engine::api::entity::entity_id(&self.table)
    }
}

impl<C> Clone for Obj<C> {
    fn clone(&self) -> Self {
        Self::new(self.table.clone())
    }
}

impl<C> Deref for Obj<C> {
    type Target = LuaTable;
    fn deref(&self) -> &LuaTable {
        &self.table
    }
}

impl<C: MohoClass> FromLua for Obj<C> {
    fn from_lua(value: LuaValue, _lua: &Lua) -> LuaResult<Self> {
        match value {
            LuaValue::Table(t) => Ok(Self::new(t)),
            other => Err(LuaError::FromLuaConversionError {
                from: other.type_name(),
                to: C::LUA_NAME.into(),
                message: Some(format!("expected a {} (moho.{}) object", C::LUA_NAME, C::CLASS)),
            }),
        }
    }
}

impl<C> IntoLua for Obj<C> {
    fn into_lua(self, _lua: &Lua) -> LuaResult<LuaValue> {
        Ok(LuaValue::Table(self.table))
    }
}

impl<C> super::ty::FieldIntoLua for Obj<C> {
    fn set_field<K: IntoLua>(&self, _lua: &Lua, t: &LuaTable, key: K) -> LuaResult<()> {
        t.raw_set(key, self.table.clone())
    }
}

impl<C: MohoClass> LuaType for Obj<C> {
    fn ty() -> LuaTy {
        LuaTy::Class(C::CLASS)
    }
}

impl<C> std::fmt::Debug for Obj<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Obj").field(&self.table).finish()
    }
}
