//! Order resolution: ends orders whose purpose is gone, before the systems
//! that execute orders (movement, construction, weapons) run.
//!
//! - Attack / OverCharge / Guard / Repair / Reclaim / Capture on an entity
//!   that no longer exists (or was destroyed) complete.
//! - Attack orders on an entity no longer hostile complete.

use mlua::prelude::*;

use crate::engine::sim::army::{Alliance, Armies};
use crate::engine::sim::command::{CommandKind as K, CommandTarget, Commands};
use crate::engine::sim::entity::{EntityId, EntityWorld};

pub fn tick(lua: &Lua) -> LuaResult<()> {
    let (Some(world), Some(mut commands)) = (lua.app_data_ref::<EntityWorld>(), lua.app_data_mut::<Commands>()) else {
        return Ok(());
    };
    let armies = lua.app_data_ref::<Armies>();
    let done: Vec<EntityId> = commands
        .units_doing(&[K::Attack, K::FormAttack, K::OverCharge, K::Guard, K::Repair, K::Reclaim, K::Capture])
        .into_iter()
        .filter(|&u| {
            let Some(cmd) = commands.current(u) else { return false };
            let CommandTarget::Entity(t) = cmd.target else { return false };
            let targets_entity = matches!(cmd.kind, K::Attack | K::FormAttack | K::OverCharge | K::Guard | K::Repair | K::Reclaim | K::Capture);
            if !targets_entity {
                return false;
            }
            let Some(target) = world.get(t).filter(|e| !e.destroyed) else { return true };
            // An attack target that became friendly is no longer a target.
            matches!(cmd.kind, K::Attack | K::FormAttack)
                && match (world.get(u).and_then(|e| e.army), target.army, &armies) {
                    (Some(a), Some(b), Some(armies)) => armies.alliance(a, b) == Alliance::Ally,
                    _ => false,
                }
        })
        .collect();
    for u in done {
        commands.pop_front(u);
    }
    Ok(())
}
