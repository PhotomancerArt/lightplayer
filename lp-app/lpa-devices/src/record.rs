//! The persisted snapshot of a device: identity plus preferences.
//!
//! **A record never describes what a device is doing.** The moment a record
//! field says "connecting" or "flashing", it is a fifth state machine with a
//! disk. Records exist so a granted port can be re-matched to a known
//! device at startup, and so a user's name and autoconnect choice survive a
//! refresh — nothing more.
//!
//! Storage is the app's problem (OPFS in Studio, files or memory
//! elsewhere): the model emits [`Command::PersistRecord`] /
//! [`Command::DeleteRecord`] and never touches a filesystem.
//!
//! [`Command::PersistRecord`]: crate::Command::PersistRecord
//! [`Command::DeleteRecord`]: crate::Command::DeleteRecord

use serde::{Deserialize, Serialize};

use crate::identity::{DeviceId, IdentityChain};
use crate::time::Millis;

/// One persisted device entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeviceRecord {
    pub device: DeviceId,
    pub identity: IdentityChain,
    /// The user's chosen name, if they set one. Distinct from
    /// [`IdentityChain::name`], which is what the device calls itself.
    pub name: Option<String>,
    pub autoconnect: bool,
    pub last_seen: Option<Millis>,
    /// The board this firmware said it was built for, from a hello's
    /// hardware facts (`HelloFacts::board_id`). `#[serde(default)]` so a
    /// registry row written before this field existed still loads.
    ///
    /// Learned, never guessed: `record_snapshot` only ever writes a `Some`
    /// it was handed, and a later fold with nothing to say leaves the
    /// stored value alone — a window reset (a reopened port, a reboot)
    /// forgets what the CURRENT observation window knows, but the record
    /// must not forget what it already learned. See
    /// [`crate::device::Device::record_snapshot`].
    #[serde(default)]
    pub board_id: Option<String>,
    /// The chip family read off a boot banner (`Evidence::detected_chip`).
    /// Same never-cleared-by-a-later-None rule as [`Self::board_id`].
    #[serde(default)]
    pub chip: Option<String>,
}

impl DeviceRecord {
    pub fn new(device: DeviceId, identity: IdentityChain) -> Self {
        Self {
            device,
            identity,
            name: None,
            autoconnect: false,
            last_seen: None,
            board_id: None,
            chip: None,
        }
    }

    /// Name to show for a device that is not currently saying anything:
    /// the user's name, else the provisioned name, else the strongest
    /// binding, else an honest placeholder.
    pub fn title(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        if let Some(name) = &self.identity.name {
            return name.clone();
        }
        self.identity
            .strongest_label()
            .unwrap_or_else(|| "Unnamed device".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{DeviceUid, EndpointKey};

    #[test]
    fn titles_prefer_the_user_name_then_the_device_name_then_a_binding() {
        let identity = IdentityChain {
            endpoint: Some(EndpointKey("usb-1".to_string())),
            uid: Some(DeviceUid("dev_abc".to_string())),
            name: Some("Studio Strip".to_string()),
            ..Default::default()
        };
        let mut record = DeviceRecord::new(DeviceId(1), identity);
        assert_eq!(record.title(), "Studio Strip");

        record.name = Some("Kitchen".to_string());
        assert_eq!(record.title(), "Kitchen");

        let anonymous = DeviceRecord::new(DeviceId(2), IdentityChain::default());
        assert_eq!(anonymous.title(), "Unnamed device");
    }

    /// A registry row written before `board_id`/`chip` existed must still
    /// load — the persisted-format rule (AGENTS.md): additive
    /// `#[serde(default)]` fields, no version bump, old rows still parse.
    #[test]
    fn a_legacy_record_without_board_id_or_chip_deserialises() {
        let json = r#"{
            "device": 1,
            "identity": {
                "endpoint": null,
                "mac": null,
                "uid": "dev000000daqf6dvvqz",
                "name": null
            },
            "name": "Kitchen",
            "autoconnect": false,
            "last_seen": null
        }"#;

        let record: DeviceRecord = serde_json::from_str(json).expect("legacy row parses");

        assert_eq!(record.board_id, None);
        assert_eq!(record.chip, None);
        assert_eq!(record.name.as_deref(), Some("Kitchen"));
    }
}
