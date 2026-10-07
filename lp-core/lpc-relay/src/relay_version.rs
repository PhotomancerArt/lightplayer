//! Version-and-refuse for the device leg.
//!
//! This is **not** `lpc-wire`'s no-compat policy. The device wire changes
//! freely because Studio, lp-cli and firmware are built and deployed
//! together. The relay has no such lockstep: a board in a lamp keeps its
//! firmware for months while lightplayer.app redeploys every day. So, like
//! `lpc-cloud-api`'s `CLOUD_API_VERSION`, the device leg carries its own
//! version, the hub accepts exactly the versions it lists, and anything
//! else is a named refusal ([`RefuseReason::VersionTooOld`] /
//! [`RefuseReason::VersionTooNew`]) the board can report — never a silent
//! decode failure, never a guessed subset.
//!
//! The version is the first field after the hello's tag, so a hub can read
//! it before it decodes anything else ([`RelayFrame::hello_version`](crate::RelayFrame::hello_version)).

use crate::refuse_reason::RefuseReason;

/// The device-leg protocol this build speaks.
///
/// Bump it on any change to [`RelayFrame`](crate::RelayFrame)'s encoding,
/// [`RefuseReason`]'s or [`RouteCloseReason`](crate::RouteCloseReason)'s
/// codes, or the proof ([`crate::relay_proof`]). Bump rarely: every bump
/// strands the boards in the field until they update, unless the hub keeps
/// the old version in [`SUPPORTED_RELAY_PROTO_VERSIONS`].
///
/// v1 = the first relay (2026-10-06).
pub const RELAY_PROTO_VERSION: u16 = 1;

/// Every version the hub accepts, oldest first. The hub keeps an old one
/// here for as long as boards in the field speak it.
pub const SUPPORTED_RELAY_PROTO_VERSIONS: &[u16] = &[RELAY_PROTO_VERSION];

/// The device leg's path on the relay's origin.
pub const RELAY_DEVICE_PATH: &str = "/relay/device";

/// The hub's verdict on a board's version: accepted, or the refusal to send.
pub fn check_relay_version(board_version: u16) -> Result<(), RefuseReason> {
    if SUPPORTED_RELAY_PROTO_VERSIONS.contains(&board_version) {
        return Ok(());
    }
    let newest = SUPPORTED_RELAY_PROTO_VERSIONS
        .iter()
        .copied()
        .max()
        .unwrap_or(RELAY_PROTO_VERSION);
    if board_version > newest {
        Err(RefuseReason::VersionTooNew)
    } else {
        Err(RefuseReason::VersionTooOld)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_version_is_accepted() {
        assert_eq!(check_relay_version(RELAY_PROTO_VERSION), Ok(()));
    }

    #[test]
    fn older_and_newer_versions_are_refused_by_name() {
        assert_eq!(check_relay_version(0), Err(RefuseReason::VersionTooOld));
        assert_eq!(
            check_relay_version(RELAY_PROTO_VERSION + 1),
            Err(RefuseReason::VersionTooNew)
        );
    }
}
