//! The device a `?on=mac:<base mac>` address names, and what it is
//! already running.
//!
//! `?on=` takes a kind or an **instance** (vision D43), and the instance
//! form names a device by the one fact that identifies it across erases,
//! renames and reflashes: its base MAC. Silicon reports one; a sim is
//! minted with one (a locally-administered `02:…`, run through the same
//! `HardwareId` derivation). So one lookup answers for both, which is the
//! whole point of "always a device".
//!
//! Two facts come back, and they come from two different places on
//! purpose:
//!
//! - **what the device is running** is the fold's, off the heartbeat
//!   ([`lpa_devices::LoadedProject`]). It is a live fact and it is
//!   anonymous: the wire carries a storage-directory label, never a
//!   `prj…` uid, so it can say *that* something is running and never
//!   *what*.
//! - **which project it was last given** is Studio's, off the registry
//!   row's [`lpc_history::DeviceAssociation`] — written when a push
//!   verifies. It names a library uid and it can be stale (someone else's
//!   browser pushed since), which is exactly why it is not trusted alone.
//!
//! The mismatch rule (D50) needs both: a device that says it is running
//! something, and an association naming a *different* project than the one
//! being opened. Either fact alone would be a guess — the heartbeat cannot
//! name the project, and the association cannot tell you the board did not
//! get erased an hour ago.

use std::collections::BTreeMap;

use lpa_devices::Device;

use crate::app::places::RegisteredDevice;

use super::sim_record::SimRecord;

/// The device an instance hint resolved to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceByBaseMac {
    /// The registry key the rest of Studio addresses this device by — its
    /// `dev…` uid, or `mac:<base mac>` for a board the firmware has not
    /// provisioned a uid onto yet.
    pub key: String,
    /// What to call it in a sentence.
    pub name: String,
    /// The library project this device was last verified to have been
    /// given (the registry association), when there is one.
    pub last_given_project: Option<String>,
    /// The device says a project is loaded right now. `false` covers both
    /// "nothing is loaded" and "it has not said" — neither is evidence of
    /// a project worth protecting.
    pub running_a_project: bool,
}

impl DeviceByBaseMac {
    /// Whether opening `project_uid` here would push over a *different*
    /// library project (D50): the device is running something, and the
    /// project it was last given is not this one.
    ///
    /// A device running something Studio cannot name (no association, or
    /// one naming a project this library does not have) answers `false`
    /// here and is handled by the caller: there is no project to offer a
    /// switch to, and no library copy standing behind what would be
    /// overwritten.
    pub fn would_push_over(&self, project_uid: &str) -> bool {
        self.running_a_project
            && self
                .last_given_project
                .as_deref()
                .is_some_and(|last| last != project_uid)
    }
}

/// The device whose base MAC is `base_mac`, or `None` when nothing this
/// library knows answers to it.
///
/// `base_mac` is expected in the canonical spelling (lowercase colon hex —
/// the router normalizes it as it parses the hint).
///
/// The roster is asked first because it holds every device Studio knows,
/// remembered boards included (the registry rehydrates into it at every
/// library settle). The sim records are the second look, for the window a
/// freshly minted sim lives in before its row settles — the same reason
/// `device_at_address` looks past the registry.
pub fn device_by_base_mac(
    devices: &[Device],
    registry: &[RegisteredDevice],
    sims: &BTreeMap<String, SimRecord>,
    base_mac: &str,
) -> Option<DeviceByBaseMac> {
    let found = devices.iter().find(|device| {
        device
            .identity
            .mac
            .as_ref()
            .is_some_and(|mac| mac.0 == base_mac)
    });
    if let Some(device) = found {
        let key = super::device_records::registry_key(&device.identity)?;
        return Some(DeviceByBaseMac {
            name: device.title(),
            last_given_project: last_given_project(registry, &key),
            running_a_project: is_running_a_project(device),
            key,
        });
    }
    // A sim minted a moment ago: the record is written, the roster row has
    // not settled. It runs nothing yet by construction, so there is
    // nothing to push over.
    let (key, _) = sims
        .iter()
        .find(|(_, record)| record.base_mac == base_mac)?;
    Some(DeviceByBaseMac {
        key: key.clone(),
        name: registry
            .iter()
            .find(|row| &row.uid == key)
            .map(|row| row.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| key.clone()),
        last_given_project: last_given_project(registry, key),
        running_a_project: false,
    })
}

/// Whether the device's heartbeat says a project is loaded on it.
///
/// `None` (never said) and `Some([])` (nothing loaded) are both "no". The
/// facts name a storage directory, not a project, so this is deliberately
/// the only question asked of them.
fn is_running_a_project(device: &Device) -> bool {
    device
        .evidence
        .loaded_projects()
        .is_some_and(|loaded| !loaded.is_empty())
}

/// The library project a registry row was last verified to have been
/// given, as a `prj…` uid string.
fn last_given_project(registry: &[RegisteredDevice], key: &str) -> Option<String> {
    registry
        .iter()
        .find(|row| row.uid == key)?
        .association
        .as_ref()
        .map(|association| association.project.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim_record(target: &str, base_mac: &str) -> SimRecord {
        SimRecord {
            version: 1,
            kind: super::super::sim_record::SIM_RECORD_KIND.to_string(),
            target: target.to_string(),
            base_mac: base_mac.to_string(),
            created_at: 0.0,
        }
    }

    fn registered(uid: &str, project: Option<&str>) -> RegisteredDevice {
        RegisteredDevice {
            uid: uid.to_string(),
            association: project.map(|project| lpc_history::DeviceAssociation {
                device: "dev000000daqf6dvvqz".parse().expect("a device uid"),
                project: project.parse().expect("a project uid"),
                version: lpc_history::ContentHash::from_bytes([0u8; 32]),
                at: 0.0,
            }),
            ..RegisteredDevice::default()
        }
    }

    /// A sim whose row has not settled yet still resolves — and it is
    /// never "running a different project", because it has not run
    /// anything.
    #[test]
    fn a_freshly_minted_sim_resolves_before_its_row_settles() {
        let sims = BTreeMap::from([(
            "dev000000daqf6dvvqz".to_string(),
            sim_record("lightplayer/desktop", "02:11:22:33:44:55"),
        )]);
        let found = device_by_base_mac(&[], &[], &sims, "02:11:22:33:44:55")
            .expect("the minted sim resolves by its MAC");
        assert_eq!(found.key, "dev000000daqf6dvvqz");
        assert!(!found.running_a_project);
        assert!(!found.would_push_over("prjh7kq9xy2mq4tb8wz"));
    }

    #[test]
    fn an_unknown_mac_resolves_to_nothing() {
        assert_eq!(
            device_by_base_mac(&[], &[], &BTreeMap::new(), "60:55:f9:0a:0b:0c"),
            None
        );
    }

    /// The mismatch rule needs BOTH facts. An association naming another
    /// project on a device that is running nothing is a board that was
    /// erased since, not a project to protect.
    #[test]
    fn pushing_over_needs_a_running_project_and_a_named_one() {
        let this = "prjh7kq9xy2mq4tb8wz";
        let other = "prj0000000000000000";
        let running_other = DeviceByBaseMac {
            key: "devx".to_string(),
            name: "Porch sign".to_string(),
            last_given_project: Some(other.to_string()),
            running_a_project: true,
        };
        assert!(running_other.would_push_over(this));
        assert!(!running_other.would_push_over(other));

        let idle = DeviceByBaseMac {
            running_a_project: false,
            ..running_other.clone()
        };
        assert!(!idle.would_push_over(this), "nothing is running to lose");

        let anonymous = DeviceByBaseMac {
            last_given_project: None,
            ..running_other.clone()
        };
        assert!(
            !anonymous.would_push_over(this),
            "a project this library cannot name has no switch to offer"
        );
    }

    /// The association is read off the row the key names, never off the
    /// first row that happens to have one.
    #[test]
    fn the_association_is_read_off_this_devices_row() {
        let registry = vec![
            registered("dev000000daqf6dvvq1", Some("prj0000000000000000")),
            registered("dev000000daqf6dvvq2", Some("prjh7kq9xy2mq4tb8wz")),
        ];
        assert_eq!(
            last_given_project(&registry, "dev000000daqf6dvvq2"),
            Some("prjh7kq9xy2mq4tb8wz".to_string())
        );
        assert_eq!(last_given_project(&registry, "devmissing"), None);
    }
}
