use mlua::prelude::*;

use crate::engine::sim::deposit::{Deposit, DepositKind, Deposits};
use crate::engine::sim::math::FxVec3;
use crate::lua_api::lua_fn;
use crate::lua_api::shapes::{List, Shaped};

/// One entry of `GetDepositsAroundPoint`'s result.
#[derive(crate::lua_api::LuaTable)]
#[lua(name = "Deposit", write_only, rename_all = "PascalCase")]
#[allow(dead_code)]
struct DepositInfo {
    x1: f64,
    x2: f64,
    z1: f64,
    z2: f64,
    /// 1 mass, 2 hydrocarbon.
    #[lua(rename = "Type")]
    kind: u8,
    /// Distance from the query point.
    dist: f64,
}

/// `GetDepositsAroundPoint(x, z, radius, type)` (FA-Binary-Patches; `type`
/// 0 all, 1 mass, 2 hydrocarbon): the deposits whose centre is within
/// `radius` (XZ) of the point, in creation order, as `{ X1, X2, Z1, Z2,
/// Type, Dist }` (the deposit's square, its type, its distance). The
/// square spans `size` cells about the centre (inferred). The interface has
/// the same over the deposits the engine hands it (`ui::user_api`).
pub fn around(lua: &Lua, deposits: &[(u8, f64, f64, f64)], x: f64, z: f64, radius: f64, kind: u8) -> LuaResult<LuaTable> {
    let out = lua.create_table()?;
    for &(k, dx, dz, size) in deposits {
        if kind != 0 && k != kind {
            continue;
        }
        let (ox, oz) = (dx - x, dz - z);
        let dist = (ox * ox + oz * oz).sqrt();
        if dist > radius {
            continue;
        }
        let t = lua.create_table_with_capacity(0, 6)?;
        let half = size / 2.0;
        t.raw_set("X1", dx - half)?;
        t.raw_set("X2", dx + half)?;
        t.raw_set("Z1", dz - half)?;
        t.raw_set("Z2", dz + half)?;
        t.raw_set("Type", k)?;
        t.raw_set("Dist", dist)?;
        out.raw_push(t)?;
    }
    Ok(out)
}

/// Deposits as (type 1 mass / 2 hydrocarbon, x, z, size).
pub fn list(deposits: &Deposits) -> Vec<(u8, f64, f64, f64)> {
    deposits
        .list
        .iter()
        .map(|d| {
            let k = match d.kind {
                DepositKind::Mass => 1,
                DepositKind::Hydrocarbon => 2,
            };
            (k, d.position.x.to_num(), d.position.z.to_num(), d.size.to_num())
        })
        .collect()
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Deposits::default());
    // The deposit list is small (tens) and the query rare (AI base
    // planning, hotkeys): a scan.
    lua_fn!(lua, global,
        /// The deposits whose centre is within `radius` (XZ) of the point, in
        /// creation order, as `{ X1, X2, Z1, Z2, Type, Dist }`. `kind` 0 or
        /// nil all, 1 mass, 2 hydrocarbon.
        GetDepositsAroundPoint = |lua, (x, z, radius, kind): (f64, f64, f64, Option<u8>)| {
            let all = lua.app_data_ref::<Deposits>().map(|d| list(&d)).unwrap_or_default();
            around(lua, &all, x, z, radius, kind.unwrap_or(0)).map(List::<Shaped<DepositInfo>>::new)
        })?;
    lua_fn!(lua, global,
        /// Adds a resource deposit (`kind` "Mass" or "Hydrocarbon") of `size`
        /// cells at the position.
        CreateResourceDeposit = 
            |lua, (kind, x, y, z, size): (String, f64, f64, f64, f64)| {
                let kind = DepositKind::parse(&kind)
                    .ok_or_else(|| LuaError::runtime(format!("unknown deposit type '{kind}'")))?;
                lua.app_data_mut::<Deposits>().expect("Deposits").list.push(Deposit {
                    kind,
                    position: FxVec3::from_f64(x, y, z),
                    size: crate::engine::sim::math::from_lua(size),
                });
                Ok(())
            })
}
