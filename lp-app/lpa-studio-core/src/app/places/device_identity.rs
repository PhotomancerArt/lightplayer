//! The on-device identity convention: `/.lp/device.json` at the device's
//! filesystem ROOT (the lpa-server base fs).
//!
//! Stamped at provisioning; USB port metadata can't distinguish identical
//! boards, so the device filesystem carries its own identity. Identity is
//! DEVICE-scoped: it lives outside every project storage dir, so project
//! pushes (which replace `projects/<storage>/`) never touch it. Stamping
//! writes the root path over the wire (`FsRequest::Write`); pulls read it
//! back the same way, and firmware reads it at boot for the hello's
//! `device_uid` (the server-side twin of this convention lives in
//! lpa-server's `device_identity` module).

use serde::{Deserialize, Serialize};

pub const DEVICE_IDENTITY_PATH: &str = "/.lp/device.json";

/// A device's stamped identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    /// `dev_…` uid.
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
            uid: "dev_0000000000000001".to_string(),
            name: "Porch sign".to_string(),
        };
        let bytes = identity.to_json_bytes();
        assert_eq!(DeviceIdentity::from_json_bytes(&bytes).unwrap(), identity);
    }
}
