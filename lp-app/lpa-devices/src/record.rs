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
}

impl DeviceRecord {
    pub fn new(device: DeviceId, identity: IdentityChain) -> Self {
        Self {
            device,
            identity,
            name: None,
            autoconnect: false,
            last_seen: None,
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
}
