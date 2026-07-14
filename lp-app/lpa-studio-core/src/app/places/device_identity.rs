//! The on-device identity convention: `/.lp/device.json`.
//!
//! Stamped at provisioning (M5's flow); USB port metadata can't distinguish
//! identical boards, so the device filesystem carries its own identity.
//! Format decided here; M5 wires the stamping and reading over the
//! protocol's fs requests.

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
