//! Which line a device is tracking: the last push it received.

use serde::{Deserialize, Serialize};

use crate::hash::content_hash::ContentHash;
use crate::uid::prefixed_uid::PrefixedUid;

/// What was last pushed to a device.
///
/// A device tracks the line last pushed to it; "behind"/"up to date" are
/// computed against *that* project's history (fleet vs family — a forked
/// variant moves the association to the fork). Data shape only: the device
/// registry that persists these lives in the places layer, not this crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceAssociation {
    pub device: PrefixedUid,
    pub project: PrefixedUid,
    pub version: ContentHash,
    /// When the push happened (f64 epoch seconds, caller-supplied).
    pub at: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uid::uid_prefix::UidPrefix;

    #[test]
    fn serde_round_trip() {
        let association = DeviceAssociation {
            device: PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]),
            project: PrefixedUid::mint(UidPrefix::Project, &[2u8; 16]),
            version: ContentHash::of(b"v"),
            at: 1700000000.0,
        };
        let json = serde_json::to_string(&association).unwrap();
        let back: DeviceAssociation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, association);
    }
}
