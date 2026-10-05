//! The install kind, **decided by hashes** (`one-way-doors.md` §5.3).
//!
//! - An offer is an **engine install** when its `core_sha256` equals the
//!   running core's hash **and** its `engine_sha256` equals the core's
//!   digest slot: the engine this core was built with. A heal (E1) and the
//!   new core fetching its own engine are both this.
//! - An offer whose `core_sha256` equals the running core's but whose
//!   `engine_sha256` does not is **self-contradictory** — one core carries
//!   one engine's digest — and is refused `N`/`H`.
//! - Any other offer is a **core install**.
//!
//! The build id never decides it: it is a label, and the input of the build
//! hash the records key on. Forever.

/// What an offer asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    /// Install the engine this core needs (by its digest slot).
    Engine,
    /// Install a new core (then its engine, from the new core).
    Core,
}

/// An offer whose two hashes cannot both be true: refused `N`/`H`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContradictoryOffer;

/// The install kind of an offer carrying `offer_core` and `offer_engine`, on
/// a core whose own hash is `own_core` and whose digest slot is
/// `digest_slot`.
pub fn install_kind(
    offer_core: &[u8; 32],
    offer_engine: &[u8; 32],
    own_core: &[u8; 32],
    digest_slot: &[u8; 32],
) -> Result<InstallKind, ContradictoryOffer> {
    if offer_core != own_core {
        return Ok(InstallKind::Core);
    }
    if offer_engine == digest_slot {
        Ok(InstallKind::Engine)
    } else {
        Err(ContradictoryOffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: [u8; 32] = [1; 32];
    const SLOT: [u8; 32] = [2; 32];
    const OTHER: [u8; 32] = [3; 32];

    #[test]
    fn the_three_cases() {
        assert_eq!(
            install_kind(&OWN, &SLOT, &OWN, &SLOT),
            Ok(InstallKind::Engine)
        );
        assert_eq!(
            install_kind(&OWN, &OTHER, &OWN, &SLOT),
            Err(ContradictoryOffer)
        );
        assert_eq!(
            install_kind(&OTHER, &SLOT, &OWN, &SLOT),
            Ok(InstallKind::Core)
        );
        assert_eq!(
            install_kind(&OTHER, &OTHER, &OWN, &SLOT),
            Ok(InstallKind::Core)
        );
    }
}
