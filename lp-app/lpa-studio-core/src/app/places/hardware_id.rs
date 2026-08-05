//! `HardwareId`: where a unit's durable identity comes from.
//!
//! Design: `~/.photomancer/planning/lp2025/2026-08-04-1748-hardware-anchored-device-identity/design.md`
//! §2/§4 (graduates to `docs/design/device-identity.md` at P5). The point
//! of this type is erase-proofness: a `dev_…` uid stamped into the
//! device's filesystem dies with `EraseDevice`, but the factory efuse MAC
//! survives it by construction, so deriving the uid FROM the MAC instead
//! of inventing one means re-flashing a board lands back on the same
//! remembered row.

use core::fmt;

use lpc_history::{PrefixedUid, UidPrefix};

/// Durable identity of a physical unit. Silicon when the transport class
/// has it; minted when it doesn't (host-class embedders, legacy/stamped
/// devices, pre-hardware-id firmware).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareId {
    /// ESP-class silicon: the factory base MAC from efuse
    /// (`HardwareFacts::base_mac` / a download-mode ROM read).
    EspEfuse { mac: [u8; 6] },
    /// Host-class embedders (`fw-host`, `lp-cli`) and legacy/stamped
    /// devices: a random `dev_` uid (today's scheme, demoted to
    /// fallback).
    Minted { uid: PrefixedUid },
}

/// Why a canonical origin string ([`HardwareId::to_string`]'s counterpart)
/// failed to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareIdParseError {
    /// Not `"minted"` and not `"efuse:…"`.
    UnknownOrigin,
    /// The `efuse:` prefix matched but the tail isn't a well-formed MAC.
    BadMac,
}

impl fmt::Display for HardwareIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            HardwareIdParseError::UnknownOrigin => {
                "hardware id origin must be \"minted\" or \"efuse:aa:bb:cc:dd:ee:ff\""
            }
            HardwareIdParseError::BadMac => "efuse origin must be followed by a colon-hex MAC",
        };
        f.write_str(msg)
    }
}

impl HardwareId {
    /// Parse the hello's `base_mac` form: lowercase colon-hex
    /// (`aa:bb:cc:dd:ee:ff`, see `lpc-wire/src/server/hello.rs`
    /// `HardwareFacts::base_mac` docs). Uppercase input is accepted and
    /// normalized; the wrong width (notably an 8-group EUI-64 — a
    /// DIFFERENT fact reported alongside `base_mac` on 802.15.4-capable
    /// chips) is rejected rather than silently truncated.
    ///
    /// The all-zero and all-ones addresses are rejected too: they are what
    /// a *failed* efuse read looks like (0x00000000 / 0xffffffff
    /// registers), and unlike malformed text they parse fine — every board
    /// whose read failed would answer to one identity. This mirrors
    /// `lpa_link::normalize_base_mac`, which applies the same rule to the
    /// download-mode (A2) reader; studio-core cannot depend that way, so
    /// the rule is stated twice on purpose and both are tested.
    pub fn from_base_mac(s: &str) -> Option<Self> {
        let mac = parse_colon_hex_mac(s)?;
        if mac.iter().all(|octet| *octet == 0x00) || mac.iter().all(|octet| *octet == 0xff) {
            return None;
        }
        Some(HardwareId::EspEfuse { mac })
    }

    /// The registry-key uid this identity resolves to (design §2, I2).
    ///
    /// `Minted` passes its uid through unchanged. `EspEfuse` derives one
    /// deterministically: `PrefixedUid::mint(Device, bytes)` where `bytes`
    /// is 16 bytes — 9 zero bytes, one `0x01` tag byte, then the 6 MAC
    /// bytes. This is a transparent embed (design §2, D2), not a salted
    /// hash: two studio installs must agree on a board's uid because the
    /// device carries it, and `mint` reduces mod 62^16 (~95 bits) so the
    /// embedded value (< 2^56) survives untouched — the body renders
    /// zero-prefixed, visibly distinct from a random mint.
    ///
    /// These derivation bytes are a G1-approved contract; do not change
    /// them without a new design decision.
    pub fn device_uid(&self) -> PrefixedUid {
        match self {
            HardwareId::Minted { uid } => *uid,
            HardwareId::EspEfuse { mac } => {
                let mut bytes = [0u8; 16];
                bytes[9] = 0x01;
                bytes[10..16].copy_from_slice(mac);
                PrefixedUid::mint(UidPrefix::Device, &bytes)
            }
        }
    }

    /// Parse the canonical origin string ([`Display`](fmt::Display)'s
    /// counterpart, e.g. from `RegisteredDevice::hardware_id`). The
    /// `Minted` arm's uid is NOT carried in the string — it's already the
    /// registry row's own key, so the origin column only records that
    /// minting happened, never repeats the value — so the caller supplies
    /// the row's uid to reconstruct it.
    pub fn parse_origin(s: &str, minted_uid: PrefixedUid) -> Result<Self, HardwareIdParseError> {
        if s == "minted" {
            return Ok(HardwareId::Minted { uid: minted_uid });
        }
        let mac_str = s
            .strip_prefix("efuse:")
            .ok_or(HardwareIdParseError::UnknownOrigin)?;
        let mac = parse_colon_hex_mac(mac_str).ok_or(HardwareIdParseError::BadMac)?;
        Ok(HardwareId::EspEfuse { mac })
    }
}

/// Canonical origin string for the registry column:
/// `"efuse:aa:bb:cc:dd:ee:ff"` or `"minted"`.
impl fmt::Display for HardwareId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HardwareId::EspEfuse { mac } => write!(
                f,
                "efuse:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ),
            HardwareId::Minted { .. } => f.write_str("minted"),
        }
    }
}

/// Parse `aa:bb:cc:dd:ee:ff` (case-insensitive) into 6 bytes. `None` for
/// any other width — in particular an 8-group EUI-64 must NOT parse as a
/// MAC.
fn parse_colon_hex_mac(s: &str) -> Option<[u8; 6]> {
    let mut mac = [0u8; 6];
    let mut groups = s.split(':');
    for slot in mac.iter_mut() {
        let group = groups.next()?;
        if group.len() != 2 {
            return None;
        }
        *slot = u8::from_str_radix(group, 16).ok()?;
    }
    if groups.next().is_some() {
        return None;
    }
    Some(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

    #[test]
    fn golden_device_uid_for_a_known_mac() {
        // Pinned so the derivation can never silently change (P1 spec).
        let id = HardwareId::EspEfuse { mac: MAC };
        assert_eq!(id.device_uid().to_string(), "dev_000000029EVDlKLX");
    }

    #[test]
    fn minted_device_uid_passes_through_unchanged() {
        let uid = PrefixedUid::mint(UidPrefix::Device, &[3u8; 16]);
        let id = HardwareId::Minted { uid };
        assert_eq!(id.device_uid(), uid);
    }

    #[test]
    fn device_uid_is_injective_across_a_one_octet_mac_difference() {
        let a = HardwareId::EspEfuse { mac: MAC };
        let mut mac_b = MAC;
        mac_b[5] = 0x00;
        let b = HardwareId::EspEfuse { mac: mac_b };
        assert_ne!(a.device_uid(), b.device_uid());
    }

    #[test]
    fn from_base_mac_parses_the_hello_form() {
        assert_eq!(
            HardwareId::from_base_mac("aa:bb:cc:dd:ee:ff"),
            Some(HardwareId::EspEfuse { mac: MAC })
        );
    }

    #[test]
    fn from_base_mac_accepts_uppercase_and_normalizes() {
        assert_eq!(
            HardwareId::from_base_mac("AA:BB:CC:DD:EE:FF"),
            Some(HardwareId::EspEfuse { mac: MAC })
        );
    }

    #[test]
    fn from_base_mac_rejects_eui64_width() {
        // an EUI-64 (802.15.4) is 8 groups, not 6 — a DIFFERENT fact from
        // base_mac (see HardwareFacts::eui64 docs) and must not parse as one.
        assert_eq!(HardwareId::from_base_mac("aa:bb:cc:dd:ee:ff:00:11"), None);
    }

    #[test]
    fn from_base_mac_rejects_the_failed_efuse_read_addresses() {
        // the shapes an efuse read returns when it FAILS: accepting either
        // would hand every failed board the same derived uid — worse than
        // leaving them anonymous (mirrors `lpa_link::normalize_base_mac`).
        assert_eq!(HardwareId::from_base_mac("00:00:00:00:00:00"), None);
        assert_eq!(HardwareId::from_base_mac("ff:ff:ff:ff:ff:ff"), None);
        assert_eq!(HardwareId::from_base_mac("FF:FF:FF:FF:FF:FF"), None);
        // one non-zero octet makes it a plausible address again
        assert!(HardwareId::from_base_mac("00:00:00:00:00:01").is_some());
    }

    #[test]
    fn from_base_mac_rejects_malformed_input() {
        assert_eq!(HardwareId::from_base_mac(""), None);
        assert_eq!(HardwareId::from_base_mac("not-a-mac"), None);
        assert_eq!(HardwareId::from_base_mac("aa:bb:cc:dd:ee"), None);
        assert_eq!(HardwareId::from_base_mac("aa:bb:cc:dd:ee:gg"), None);
    }

    #[test]
    fn canonical_origin_round_trips_for_efuse() {
        let id = HardwareId::EspEfuse { mac: MAC };
        let s = id.to_string();
        assert_eq!(s, "efuse:aa:bb:cc:dd:ee:ff");
        let unused_uid = PrefixedUid::mint(UidPrefix::Device, &[0u8; 16]);
        assert_eq!(HardwareId::parse_origin(&s, unused_uid).unwrap(), id);
    }

    #[test]
    fn canonical_origin_for_minted_is_a_bare_marker() {
        // the string never carries the uid (design §4: it's already the
        // row key) — display is constant, and parsing needs the row's uid
        // supplied back in.
        let uid = PrefixedUid::mint(UidPrefix::Device, &[9u8; 16]);
        let id = HardwareId::Minted { uid };
        assert_eq!(id.to_string(), "minted");
        assert_eq!(HardwareId::parse_origin("minted", uid).unwrap(), id);
    }

    #[test]
    fn parse_origin_rejects_unknown_strings() {
        let uid = PrefixedUid::mint(UidPrefix::Device, &[0u8; 16]);
        assert_eq!(
            HardwareId::parse_origin("bogus", uid),
            Err(HardwareIdParseError::UnknownOrigin)
        );
        assert_eq!(
            HardwareId::parse_origin("efuse:not-a-mac", uid),
            Err(HardwareIdParseError::BadMac)
        );
    }
}
