//! R2 — the capability boundary the setup machine branches on.
//!
//! Design: `docs/design/device-setup-flow.md` §6 R2. The machine never asks
//! "is this the simulator". It asks three questions:
//!
//! - `needs_connect` — is there a port to grant and a board to probe?
//! - `needs_flash` — does firmware have to be written before provisioning?
//! - `can_rename` — does this target carry a user-visible device name?
//!
//! That is what lets the M9 wasm fake (multi-device roadmap) drive the FULL
//! hardware path in tests: it answers yes to all three, and every state the
//! hardware path visits is reachable with no hardware attached.

/// The three capabilities, as a plain value the pure reducer can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupCapabilities {
    /// A port must be granted and the board probed before a board pick.
    pub needs_connect: bool,
    /// Firmware must be flashed before the project is provisioned.
    pub needs_flash: bool,
    /// The target carries a device name the user may set at provision.
    pub can_rename: bool,
}

impl SetupCapabilities {
    /// A board on the end of a wire: connect, flash, name.
    pub const HARDWARE: Self = Self {
        needs_connect: true,
        needs_flash: true,
        can_rename: true,
    };

    /// The single simulator session: none of the three. It is not "the sim
    /// path" to the machine — it is a target that answers no.
    pub const SIMULATOR: Self = Self {
        needs_connect: false,
        needs_flash: false,
        can_rename: false,
    };
}

/// What the setup flow is setting up.
///
/// Implementors describe capabilities only. Nothing here performs I/O — a
/// target is a description, and the executor
/// ([`dispatch_for`](super::dispatch_for)) is what talks to machinery.
pub trait SetupTarget {
    fn capabilities(&self) -> SetupCapabilities;

    /// The card key ops address this target with
    /// ([`DeviceTarget::card`](crate::DeviceTarget::card)). `None` before a
    /// session exists — the hardware path has no card key until the port is
    /// granted.
    fn card_key(&self) -> Option<&str> {
        None
    }
}

/// A physical board reached over a serial link.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareSetupTarget {
    /// The card key once a session exists; `None` during CONNECT_INTRO.
    pub card_key: Option<String>,
}

impl SetupTarget for HardwareSetupTarget {
    fn capabilities(&self) -> SetupCapabilities {
        SetupCapabilities::HARDWARE
    }

    fn card_key(&self) -> Option<&str> {
        self.card_key.as_deref()
    }
}

/// THE simulator session (`runtime-sim`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatorSetupTarget {
    pub card_key: String,
}

impl Default for SimulatorSetupTarget {
    fn default() -> Self {
        Self {
            card_key: "runtime-sim".to_string(),
        }
    }
}

impl SetupTarget for SimulatorSetupTarget {
    fn capabilities(&self) -> SetupCapabilities {
        SetupCapabilities::SIMULATOR
    }

    fn card_key(&self) -> Option<&str> {
        Some(&self.card_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_answers_yes_to_all_three() {
        let caps = HardwareSetupTarget::default().capabilities();
        assert!(caps.needs_connect);
        assert!(caps.needs_flash);
        assert!(caps.can_rename);
    }

    #[test]
    fn the_simulator_answers_no_to_all_three() {
        let caps = SimulatorSetupTarget::default().capabilities();
        assert!(!caps.needs_connect);
        assert!(!caps.needs_flash);
        assert!(!caps.can_rename);
    }

    #[test]
    fn a_target_is_described_by_capabilities_not_by_kind() {
        // The point of R2: a fake that WANTS the hardware path takes the
        // hardware capabilities and the machine cannot tell the difference.
        struct WasmFake;
        impl SetupTarget for WasmFake {
            fn capabilities(&self) -> SetupCapabilities {
                SetupCapabilities::HARDWARE
            }
        }
        assert_eq!(
            WasmFake.capabilities(),
            HardwareSetupTarget::default().capabilities()
        );
    }
}
