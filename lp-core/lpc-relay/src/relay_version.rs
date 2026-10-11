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
//!
//! Two versions exist, and the hub accepts both:
//!
//! - **Protocol 1** ([`RELAY_PROTO_1`]), the first relay (2026-10-06):
//!   tags `0x01`–`0x09`. Cores in the field speak it, so the hub accepts it
//!   **forever** and never sends a protocol 1 board a frame protocol 1 does
//!   not have. Its bytes are pinned by `tests/relay_frame_golden.rs`, which
//!   is never edited.
//! - **Protocol 2** ([`RELAY_PROTO_2`]), pictures through the cloud
//!   (2026-10-08): the hello's firmware tail and tags `0x0a`–`0x0c`
//!   (`Project`, `Picture`, `PictureRate`). Its bytes are pinned by
//!   `tests/relay_frame_golden_v2.rs`; once a core that speaks it is
//!   released, it is never-break too.
//!
//! Which protocol a frame kind belongs to is
//! [`RelayFrame::protocol`](crate::RelayFrame::protocol).

use crate::refuse_reason::RefuseReason;

/// Relay protocol 1: the first relay (2026-10-06). Fielded cores speak it;
/// the hub accepts it forever (the cloud relay ADR's 2026-10-08 amendment).
pub const RELAY_PROTO_1: u16 = 1;

/// Relay protocol 2: pictures through the cloud (2026-10-08): the hello's
/// firmware, `Project`, `Picture`, `PictureRate`.
pub const RELAY_PROTO_2: u16 = 2;

/// The protocol a board built from this tree speaks.
///
/// A change to [`RelayFrame`](crate::RelayFrame)'s encoding,
/// [`RefuseReason`]'s or [`RouteCloseReason`](crate::RouteCloseReason)'s
/// codes, or the proof ([`crate::relay_proof`]) is a **new** protocol,
/// added beside the old ones: the old protocol's bytes stay exactly as they
/// are, the hub keeps it in [`SUPPORTED_RELAY_PROTO_VERSIONS`], and never
/// sends its boards a frame it does not have. Never an edit of a released
/// protocol's bytes.
pub const RELAY_PROTO_VERSION: u16 = RELAY_PROTO_2;

/// Every version the hub accepts, oldest first. A version leaves this list
/// only by a decision of its own, never a deploy.
pub const SUPPORTED_RELAY_PROTO_VERSIONS: &[u16] = &[RELAY_PROTO_1, RELAY_PROTO_2];

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

    /// The never-break property: fielded cores speak protocol 1, and a
    /// deploy must never strand them.
    #[test]
    fn protocol_1_is_accepted_forever() {
        assert_eq!(check_relay_version(1), Ok(()));
        assert!(SUPPORTED_RELAY_PROTO_VERSIONS.contains(&RELAY_PROTO_1));
        assert_eq!(RELAY_PROTO_1, 1);
    }

    #[test]
    fn protocol_2_is_accepted() {
        assert_eq!(check_relay_version(2), Ok(()));
        assert_eq!(RELAY_PROTO_2, 2);
        assert_eq!(RELAY_PROTO_VERSION, RELAY_PROTO_2);
        assert_eq!(SUPPORTED_RELAY_PROTO_VERSIONS, [1, 2]);
    }

    #[test]
    fn three_is_too_new() {
        assert_eq!(check_relay_version(3), Err(RefuseReason::VersionTooNew));
    }

    #[test]
    fn zero_is_too_old() {
        assert_eq!(check_relay_version(0), Err(RefuseReason::VersionTooOld));
    }
}
