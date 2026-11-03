//! Moho unit states (`IsUnitState` / `SetUnitState`), as a bit set.

/// State names in bit order (`/engine/Sim/Unit.lua` `UnitState`).
pub const NAMES: [&str; 46] = [
    "Immobile", "Moving", "Attacking", "Guarding", "Building", "Upgrading", "WaitingForTransport", "TransportLoading", "TransportUnloading", "MovingDown", "MovingUp", "Patrolling", "Busy", "Attached", "BeingReclaimed", "Repairing", "Diving", "Surfacing", "Teleporting", "Ferrying", "WaitForFerry", "AssistMoving", "PathFinding", "ProblemGettingToGoal", "NeedToTerminateTask", "Capturing", "BeingCaptured", "Reclaiming", "AssistingCommander", "Refueling", "GuardBusy", "ForceSpeedThrough", "UnSelectable", "DoNotTarget", "LandingOnPlatform", "CannotFindPlaceToLand", "BeingUpgraded", "Enhancing", "BeingBuilt", "NoReclaim", "NoCost", "BlockCommandQueue", "MakingAttackRun", "HoldingPattern", "SiloBuildingAmmo", "Sacrificing",
];

/// Command caps in bit order (`/engine/Enums.lua` `CommandCap`).
pub const COMMAND_CAPS: [&str; 25] = [
    "RULEUCC_Move", "RULEUCC_Stop", "RULEUCC_Attack", "RULEUCC_Guard", "RULEUCC_Patrol", "RULEUCC_RetaliateToggle", "RULEUCC_Repair", "RULEUCC_Capture", "RULEUCC_Transport", "RULEUCC_CallTransport", "RULEUCC_Nuke", "RULEUCC_Tactical", "RULEUCC_Teleport", "RULEUCC_Ferry", "RULEUCC_SiloBuildTactical", "RULEUCC_SiloBuildNuke", "RULEUCC_Sacrifice", "RULEUCC_Pause", "RULEUCC_Overcharge", "RULEUCC_Dive", "RULEUCC_Reclaim", "RULEUCC_SpecialAction", "RULEUCC_Dock", "RULEUCC_Script", "RULEUCC_Invalid",
];

/// Toggle caps in bit order (`/engine/Enums.lua` `EngineToggleCap`).
pub const TOGGLE_CAPS: [&str; 9] = [
    "RULEUTC_ShieldToggle", "RULEUTC_WeaponToggle", "RULEUTC_JammingToggle", "RULEUTC_IntelToggle", "RULEUTC_ProductionToggle", "RULEUTC_StealthToggle", "RULEUTC_GenericToggle", "RULEUTC_SpecialToggle", "RULEUTC_CloakToggle",
];

/// Bit of a cap name in `names` (exact match).
pub fn cap_bit(names: &[&str], name: &str) -> Option<u32> {
    names.iter().position(|n| *n == name).map(|i| 1u32 << i)
}

/// A bitfield blueprint field as moho writes it back into the Lua table at
/// registration (`SCR_RObjectToLuaMerge`, MohoEngine.dll 0x100c0810, via the
/// type's text form): the name when exactly one bit is set, else the
/// decimal number as a string (`'Land'`, `'RULEUTC_JammingToggle'`,
/// `'575'`, `'0'`). Real-game probe `probe_bp`.
pub fn bitfield_text(names: &[&str], bits: u32) -> String {
    match bits.is_power_of_two().then(|| names.get(bits.trailing_zeros() as usize)).flatten() {
        Some(name) => (*name).to_string(),
        None => bits.to_string(),
    }
}

/// The bits of a bitfield blueprint field: the registered text form
/// ([`bitfield_text`]), a number, or a blueprint's own `{ NAME = true }`
/// table (before registration). Table keys may carry `prefix`
/// (`LAYER_Land`). Unknown names are ignored.
pub fn bitfield_bits(names: &[&str], prefix: &str, v: &mlua::Value) -> mlua::Result<u32> {
    let named = |s: &str| cap_bit(names, s.strip_prefix(prefix).unwrap_or(s)).unwrap_or(0);
    Ok(match v {
        mlua::Value::Integer(n) => *n as u32,
        mlua::Value::Number(n) => *n as u32,
        mlua::Value::String(s) => {
            let s = s.to_str()?;
            s.parse::<u32>().unwrap_or_else(|_| named(&s))
        }
        mlua::Value::Table(t) => {
            let mut bits = 0;
            for pair in t.pairs::<mlua::Value, mlua::Value>() {
                if let (mlua::Value::String(k), mlua::Value::Boolean(true)) = pair? {
                    bits |= named(&k.to_str()?);
                }
            }
            bits
        }
        _ => 0,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnitStates(u64);

impl UnitStates {
    /// Bit index of a state name (exact match, as moho).
    pub fn index(name: &str) -> Option<u32> {
        NAMES.iter().position(|n| *n == name).map(|i| i as u32)
    }

    /// Bit mask of several states (look it up once, not per tick).
    pub fn mask(names: &[&str]) -> u64 {
        names.iter().filter_map(|n| Self::index(n)).fold(0, |m, i| m | 1 << i)
    }

    /// Whether any state of `mask` is set.
    pub fn any(self, mask: u64) -> bool {
        self.0 & mask != 0
    }

    pub fn contains(self, index: u32) -> bool {
        self.0 & (1 << index) != 0
    }

    pub fn set(&mut self, index: u32, on: bool) {
        if on {
            self.0 |= 1 << index;
        } else {
            self.0 &= !(1 << index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitfields_read_back_their_text() {
        let lua = mlua::Lua::new();
        for bits in [0, 1, 4, 575, 4194879] {
            let text = bitfield_text(&COMMAND_CAPS, bits);
            assert_eq!(bitfield_bits(&COMMAND_CAPS, "", &mlua::Value::String(lua.create_string(&text).unwrap())).unwrap(), bits, "{text}");
        }
        assert_eq!(bitfield_text(&COMMAND_CAPS, 4), "RULEUCC_Attack");
        assert_eq!(bitfield_bits(&COMMAND_CAPS, "", &mlua::Value::Integer(9)).unwrap(), 9);
    }

    #[test]
    fn set_and_query_by_name() {
        let mut s = UnitStates::default();
        let busy = UnitStates::index("Busy").unwrap();
        s.set(busy, true);
        assert!(s.contains(busy));
        assert!(!s.contains(UnitStates::index("Moving").unwrap()));
        s.set(busy, false);
        assert_eq!(s, UnitStates::default());
        assert_eq!(UnitStates::index("busy"), None);
    }
}
