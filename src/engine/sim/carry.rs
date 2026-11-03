//! Whether a transport can carry a unit: one rule for the sim (the per-unit
//! order check, boarding) and the interface (the right-click load order).
//!
//! moho's `CAiTransportImpl::TransportCanCarryUnit` (MohoEngine.dll
//! 0x101d3140): the unit must be mobile (`IsMobile`); an air-staging
//! transport (`TransportIsAirStagingPlatform`, impl +0x18) takes only air
//! units (`Unit::IsAirUnit`, +0x67b), any other only units that aren't; a
//! `COMMAND` unit needs a `CANTRANSPORTCOMMANDER` transport; and the unit's
//! `Transport.TransportClass` must fit (moho counts the blueprint's
//! `ClassGenericUpTo` / `Class2AttachSize` / `Class3AttachSize` /
//! `ClassSAttachSize` slots; ours, as the transport code boards, the
//! classes of the model's attach-point bones; carriers and air staging
//! take any class into storage).

/// The passenger's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passenger {
    pub mobile: bool,
    pub air: bool,
    pub commander: bool,
    /// `Transport.TransportClass`, 1..3.
    pub class: u8,
}

/// The transport's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carrier {
    /// Has a transport interface at all (moho's `Unit` +0x54c, made in the
    /// `Unit` constructor, 0x10285a49..0x10285aad, when its command caps
    /// hold `RULEUCC_Transport` or it's a `PODSTAGINGPLATFORM`).
    pub transport: bool,
    /// `AIRSTAGINGPLATFORM`.
    pub staging: bool,
    /// `CANTRANSPORTCOMMANDER`.
    pub carries_commander: bool,
    /// Bit `class - 1` for each class it has an attach point for.
    pub slot_classes: u8,
    /// Takes units into storage (`Transport.StorageSlots`), any class.
    pub storage: bool,
}

pub fn can_carry(unit: &Passenger, transport: &Carrier) -> bool {
    let class = unit.class.clamp(1, 3);
    transport.transport
        && unit.mobile
        && transport.staging == unit.air
        && (!unit.commander || transport.carries_commander)
        && (transport.staging || transport.storage || transport.slot_classes & (1 << (class - 1)) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TANK: Passenger = Passenger { mobile: true, air: false, commander: false, class: 1 };
    const TRANSPORT: Carrier = Carrier { transport: true, staging: false, carries_commander: false, slot_classes: 0b001, storage: false };

    #[test]
    fn moho_carry_rule() {
        assert!(can_carry(&TANK, &TRANSPORT));
        assert!(!can_carry(&TANK, &Carrier { transport: false, ..TRANSPORT }), "no transport interface");
        assert!(!can_carry(&Passenger { mobile: false, ..TANK }, &TRANSPORT), "a structure");
        assert!(!can_carry(&Passenger { air: true, ..TANK }, &TRANSPORT), "an aircraft onto a transport");
        assert!(can_carry(&Passenger { air: true, ..TANK }, &Carrier { staging: true, slot_classes: 0, ..TRANSPORT }), "an aircraft onto air staging");
        assert!(!can_carry(&TANK, &Carrier { staging: true, ..TRANSPORT }), "a tank onto air staging");
        assert!(!can_carry(&Passenger { commander: true, ..TANK }, &TRANSPORT), "a commander");
        assert!(can_carry(&Passenger { commander: true, ..TANK }, &Carrier { carries_commander: true, ..TRANSPORT }));
        assert!(!can_carry(&Passenger { class: 3, ..TANK }, &TRANSPORT), "no large slot");
        assert!(can_carry(&Passenger { class: 3, ..TANK }, &Carrier { storage: true, ..TRANSPORT }), "storage takes any class");
    }
}
