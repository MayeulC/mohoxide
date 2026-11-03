//! Resource deposits (mass / hydrocarbon) placed at session start.

use crate::engine::sim::math::{Fx, FxVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DepositKind {
    Mass,
    Hydrocarbon,
}

impl DepositKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Mass" => Some(Self::Mass),
            "Hydrocarbon" => Some(Self::Hydrocarbon),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Mass => "Mass",
            Self::Hydrocarbon => "Hydrocarbon",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Deposit {
    pub kind: DepositKind,
    pub position: FxVec3,
    /// Footprint size in cells (1 for mass, 3 for hydro on stock maps).
    pub size: Fx,
}

impl Deposit {
    /// The cells it covers, (x0, z0, x1, z1): `size` cells across from cell
    /// round(position - size / 2), halves to even (`CreateResourceDeposit`,
    /// MohoEngine.dll 0x10133c60). A size-1 deposit at a whole number takes
    /// the even cell of the two it straddles.
    pub fn cells(&self) -> (i64, i64, i64, i64) {
        let size = self.size.round().to_num::<i64>();
        let x0 = crate::engine::sim::nav::round_half_even(self.position.x - self.size / 2);
        let z0 = crate::engine::sim::nav::round_half_even(self.position.z - self.size / 2);
        (x0, z0, x0 + size, z0 + size)
    }
}

/// All deposits in creation order (deterministic iteration for queries).
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct Deposits {
    pub list: Vec<Deposit>,
}
