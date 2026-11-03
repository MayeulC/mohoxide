//! Capture: units executing `Capture` orders on an enemy unit.
//!
//! Per tick (after movement has brought the captor into build range):
//! 1. Start: `OnStartCapture(target)` on the captor (`Capturing` state),
//!    `OnStartBeingCaptured(captor)` on the target (`BeingCaptured`).
//! 2. Progress: `time` from the captor's `GetCaptureCosts(target)` (FAF's
//!    Lua, per tick); progress += `dt / time`, shown as the captor's work
//!    progress.
//! 3. Finish (progress 1): `OnStopCapture(target)` / `OnStopBeingCaptured
//!    (captor)`, then `target:OnCaptured(captor)` (FAF's script transfers it
//!    with `ChangeUnitArmy`), and the order pops.
//! A captor whose order changed, or that left range, fails:
//! `OnFailedCapture(target)` / `OnFailedBeingCaptured(captor)`, progress
//! lost. Uncapturable targets (`SetCapturable(false)`) and targets with no
//! capture time (`GetCaptureCosts` 0: unfinished units) end the order.
//! Captors are kept by id, so callbacks run in a fixed order.
//!
//! Inferred, not verified against the binary: the energy cost isn't charged
//! (and so doesn't slow capture when short), each captor progresses on its
//! own (moho combines several captors), range = build range + 2.

use std::collections::BTreeMap;

use mlua::prelude::*;

use crate::engine::api::callbacks::UnitCallbacks;
use crate::engine::api::entity;
use crate::lua_api::Obj;
use crate::engine::sim::command::{CommandId, CommandKind, CommandTarget, Commands};
use crate::engine::sim::entity::{EntityId, EntityWorld};
use crate::engine::sim::math::Fx;
use crate::engine::sim::unit_state::UnitStates;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Job {
    target: EntityId,
    command: CommandId,
    progress: Fx,
}

/// Captors and their capture in progress.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Captures(BTreeMap<EntityId, Job>);

fn set_state(lua: &Lua, unit: EntityId, name: &str, on: bool) {
    let i = UnitStates::index(name).expect("unit state");
    if let Some(u) = entity::world(lua).get_mut(unit).and_then(|e| e.unit.as_deref_mut()) {
        u.states.set(i, on);
    }
}

fn set_work(lua: &Lua, unit: EntityId, focus: Option<EntityId>, progress: Fx) -> LuaResult<()> {
    if let Some(u) = entity::world(lua).get_mut(unit).and_then(|e| e.unit.as_deref_mut()) {
        u.work_progress = progress;
    }
    crate::engine::api::unit::set_focus(lua, unit, focus)
}

fn pop_order(lua: &Lua, unit: EntityId, command: CommandId) {
    let mut c = lua.app_data_mut::<Commands>().expect("Commands");
    if c.current(unit).is_some_and(|cmd| cmd.id == command) {
        c.pop_front(unit);
    }
}

fn in_reach(world: &EntityWorld, captor: EntityId, target: EntityId) -> bool {
    let (Some(c), Some(t)) = (world.get(captor).filter(|e| !e.destroyed), world.get(target).filter(|e| !e.destroyed)) else { return false };
    // The range touches the target's build skirt (a large structure is
    // captured from its edge, its centre needn't come in range).
    t.in_build_range(c.position(), c.unit.as_deref().map_or(Fx::from_num(5), |u| u.build_range) + Fx::from_num(2))
}

/// Ends a capture: `OnStop*` when it succeeded, `OnFailed*` otherwise.
fn end(lua: &Lua, captor: EntityId, success: bool) -> LuaResult<()> {
    let Some(job) = lua.app_data_mut::<Captures>().expect("Captures").0.remove(&captor) else { return Ok(()) };
    set_work(lua, captor, None, Fx::ZERO)?;
    set_state(lua, captor, "Capturing", false);
    set_state(lua, job.target, "BeingCaptured", false);
    let (c_obj, t_obj) = (entity::object(lua, captor)?, entity::object(lua, job.target)?);
    if let (Some(c), Some(t)) = (&c_obj, &t_obj) {
        if success {
            UnitCallbacks::on_stop_capture(lua, c, "unit", Obj::new(t.clone()))?;
            UnitCallbacks::on_stop_being_captured(lua, t, "unit", Obj::new(c.clone()))?;
        } else {
            UnitCallbacks::on_failed_capture(lua, c, "unit", Obj::new(t.clone()))?;
            UnitCallbacks::on_failed_being_captured(lua, t, "unit", Obj::new(c.clone()))?;
        }
    }
    if let Some(c) = &c_obj
        && entity::world(lua).get(captor).is_some_and(|e| !e.destroyed) {
        crate::engine::api::construction::stop_tracking(lua, c)?;
    }
    Ok(())
}

/// `GetCaptureCosts(target)` on the captor: time in seconds (0 = can't).
fn capture_time(captor: &LuaTable, target: &LuaTable) -> LuaResult<Fx> {
    let Some(f) = captor.get::<Option<LuaFunction>>("GetCaptureCosts")? else { return Ok(Fx::ZERO) };
    match f.call::<(Option<f64>, Option<f64>, Option<f64>)>((captor.clone(), target.clone())) {
        Ok((t, _, _)) => Ok(crate::engine::sim::math::from_lua(t.unwrap_or(0.0))),
        Err(e) => {
            tracing::warn!(target: "lua", "Error running GetCaptureCosts script in unit: {e}");
            Ok(Fx::ZERO)
        }
    }
}

pub fn tick(lua: &Lua) -> LuaResult<()> {
    let orders: Vec<(EntityId, EntityId, CommandId)> = {
        let Some(c) = lua.app_data_ref::<Commands>() else { return Ok(()) };
        c.units_doing(&[CommandKind::Capture])
            .into_iter()
            .filter_map(|u| {
                let cmd = c.current(u)?;
                match (cmd.kind, cmd.target) {
                    (CommandKind::Capture, CommandTarget::Entity(t)) => Some((u, t, cmd.id)),
                    _ => None,
                }
            })
            .collect()
    };
    // Captures whose order is no longer current fail.
    let jobs: Vec<(EntityId, Job)> = lua.app_data_ref::<Captures>().map(|c| c.0.iter().map(|(&k, &v)| (k, v)).collect()).unwrap_or_default();
    for (captor, job) in jobs {
        let current = lua.app_data_ref::<Commands>().and_then(|c| c.current(captor).map(|c| c.id));
        if current != Some(job.command) {
            end(lua, captor, false)?;
        }
    }
    let dt = Fx::ONE / Fx::from_num(10);
    for (captor, target, command) in orders {
        let capturable = entity::world(lua).get(target).is_some_and(|t| t.unit.as_deref().is_some_and(|u| u.capturable));
        if !capturable {
            end(lua, captor, false)?;
            pop_order(lua, captor, command);
            continue;
        }
        let current = lua.app_data_ref::<Captures>().and_then(|c| c.0.get(&captor).copied());
        if !in_reach(&entity::world(lua), captor, target) {
            if current.is_some() {
                end(lua, captor, false)?;
            }
            continue;
        }
        let (Some(c_obj), Some(t_obj)) = (entity::object(lua, captor)?, entity::object(lua, target)?) else { continue };
        let time = capture_time(&c_obj, &t_obj)?;
        if time <= Fx::ZERO {
            end(lua, captor, false)?;
            pop_order(lua, captor, command);
            continue;
        }
        let mut job = match current {
            Some(j) => j,
            None => {
                let job = Job { target, command, progress: Fx::ZERO };
                lua.app_data_mut::<Captures>().expect("Captures").0.insert(captor, job);
                set_work(lua, captor, Some(target), Fx::ZERO)?;
                set_state(lua, captor, "Capturing", true);
                set_state(lua, target, "BeingCaptured", true);
                crate::engine::api::construction::prepare_arm(lua, &c_obj)?;
                UnitCallbacks::on_start_capture(lua, &c_obj, "unit", Obj::new(t_obj.clone()))?;
                UnitCallbacks::on_start_being_captured(lua, &t_obj, "unit", Obj::new(c_obj.clone()))?;
                job
            }
        };
        if entity::world(lua).get(captor).and_then(|e| e.unit.as_deref().map(|u| u.paused)).unwrap_or(false) {
            continue;
        }
        job.progress = (job.progress + dt / time).min(Fx::ONE);
        set_work(lua, captor, Some(target), job.progress)?;
        if job.progress < Fx::ONE {
            if let Some(j) = lua.app_data_mut::<Captures>().expect("Captures").0.get_mut(&captor) {
                j.progress = job.progress;
            }
            continue;
        }
        end(lua, captor, true)?;
        UnitCallbacks::on_captured(lua, &t_obj, "unit", Obj::new(c_obj.clone()))?;
        pop_order(lua, captor, command);
    }
    Ok(())
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Captures::default());
    Ok(())
}
