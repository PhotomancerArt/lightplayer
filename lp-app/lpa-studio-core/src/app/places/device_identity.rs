//! The LEGACY on-device identity convention: `/.lp/device.json` at the
//! device's filesystem ROOT (the lpa-server base fs).
//!
//! This file used to be where a device's identity came from — a `dev`
//! uid Studio minted and stamped at provisioning. Since
//! `docs/adr/2026-08-04-device-identity-anchored-in-silicon.md` an
//! ESP-class board's identity is its factory efuse MAC, and this file
//! has three narrower jobs:
//!
//! - **Read, always** (rule A3): a board carrying a stamp is evidence,
//!   both for pre-MAC firmware and for migrating a legacy registry row to
//!   the derived uid at first sight.
//! - **Store, host-class only** (D3): `fw-host`/`lp-cli` embedders have
//!   no efuse, and a host filesystem is not erased by a flash tool, so
//!   the file is an honest store there.
//! - **Written only as that fallback**: ESP-class provisioning and renames
//!   write the registry alone (design §5) — nothing is stamped onto a
//!   board whose name would die with the next erase.
//!
//! Identity is DEVICE-scoped: the file lives outside every project storage
//! dir, so project pushes (which replace `projects/<storage>/`) never
//! touch it. The fallback write goes over the wire (`FsRequest::Write`);
//! pulls read it back the same way, and firmware reads it at boot for the
//! hello's `device_uid` (the server-side twin of this convention lives in
//! lpa-server's `device_identity` module).

use serde::{Deserialize, Serialize};

pub const DEVICE_IDENTITY_PATH: &str = "/.lp/device.json";

/// The device-side hardware-manifest override, at the fs ROOT like the
/// identity above. The firmware's boot loader
/// (`fw-esp32-common/src/hardware/manifest_loader.rs`) reads this path;
/// provisioning writes the chosen board's runtime manifest here (D4), and
/// the pin map takes effect on the NEXT boot. Absent = the compiled-in
/// per-target default stands.
pub const DEVICE_HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// A device's identity as the file spells it — and, in memory, the
/// resolved identity a live session wears (uid from silicon or a legacy
/// stamp; name from the registry, D34).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    /// `dev…` uid.
    pub uid: String,
    /// Human name, gently insisted on at provisioning ("Luna's porch sign").
    pub name: String,
}

impl DeviceIdentity {
    pub fn to_json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).expect("device identity serializes")
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let identity = DeviceIdentity {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
        };
        let bytes = identity.to_json_bytes();
        assert_eq!(DeviceIdentity::from_json_bytes(&bytes).unwrap(), identity);
    }
}
