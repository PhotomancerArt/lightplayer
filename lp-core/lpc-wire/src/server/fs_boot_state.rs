//! How the unit's filesystem came up at boot — a hello fact.
//!
//! Studio needs it for two things the rest of the hello cannot say
//! (plan `lp2025/2026-10-01-1843-c6-repartition`, MQ3): that a board is
//! **holding** a pre-repartition filesystem it refused to format over
//! ([`FsBootState::LegacyHeld`] — "Finish update"), and that a migration's
//! files really mounted afterwards ([`FsBootState::Mounted`], not a fresh
//! [`FsBootState::Formatted`] filesystem that only happens to boot).

use serde::{Deserialize, Serialize};

/// How the filesystem the server is serving came up at boot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsBootState {
    /// An existing flash filesystem mounted as it was. A host server whose
    /// filesystem is a directory on disk reports this too: what it serves
    /// persisted from before.
    Mounted,
    /// No filesystem mounted, so the flash region was formatted fresh and
    /// mounted empty. Whatever it held before is gone.
    Formatted,
    /// The server is serving a RAM filesystem: no `lpfs` partition in the
    /// flashed table, a `memory_fs` build, a host/browser/emulator server
    /// with no persistent storage, or a flash filesystem that would neither
    /// mount nor format. Nothing written survives a reset.
    ///
    /// The `Default` — the honest report for an embedder that never says
    /// otherwise.
    #[default]
    Memory,
    /// The flash filesystem would not mount at its partition, and a
    /// LightPlayer filesystem in the **pre-repartition** layout was found
    /// where it used to be, so the firmware refused to format (formatting
    /// would destroy it) and is serving a RAM filesystem instead. The
    /// board's files are intact and waiting for a migration (Studio's
    /// Update firmware, or `lp-cli hardware lpfs migrate`).
    LegacyHeld,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_rides_the_wire_as_its_snake_case_name() {
        for (state, name) in [
            (FsBootState::Mounted, "\"mounted\""),
            (FsBootState::Formatted, "\"formatted\""),
            (FsBootState::Memory, "\"memory\""),
            (FsBootState::LegacyHeld, "\"legacy_held\""),
        ] {
            let json = crate::json::to_string(&state).unwrap();
            assert_eq!(json, name);
            let back: FsBootState = crate::json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }
}
