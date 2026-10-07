//! What the station knows of the board's network file: no passwords.

use alloc::string::String;
use alloc::vec::Vec;

/// The part of `/.lp/network.json` the join policy decides with: the Wi-Fi
/// switch and each saved network's name, whether it is hidden, and a
/// fingerprint of its password. **Never the password** — the station task
/// looks it up in the file it read when the policy asks it to connect.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StationSettings {
    /// The board's Wi-Fi switch.
    pub wifi: bool,
    /// The saved networks, in the file's order (the order does not rank
    /// them: the join rule picks the strongest heard).
    pub networks: Vec<SavedNetwork>,
}

/// One saved network as the policy sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedNetwork {
    /// The network name.
    pub ssid: String,
    /// It does not broadcast its name: tried by name after the heard ones.
    pub hidden: bool,
    /// A fingerprint of the saved password ([`secret_tag`]), so a changed
    /// password is noticed without the policy holding it.
    pub secret_tag: u32,
}

impl StationSettings {
    /// What the policy may know of `file`: everything but the passwords,
    /// each replaced by its [`secret_tag`].
    #[must_use]
    pub fn from_file(file: &lpc_access::NetworkFile) -> Self {
        Self {
            wifi: file.wifi,
            networks: file
                .networks
                .iter()
                .map(|network| SavedNetwork {
                    ssid: network.ssid.clone(),
                    hidden: network.hidden,
                    secret_tag: secret_tag(&network.password),
                })
                .collect(),
        }
    }

    /// "Set to use Wi-Fi" (plan Q2): the switch is on **and** at least one
    /// network is saved. While it holds the Radio node is off, so the
    /// station may scan and hop channels; with nothing saved the station
    /// never scans and ESP-NOW keeps its channel.
    #[must_use]
    pub fn uses_wifi(&self) -> bool {
        self.wifi && !self.networks.is_empty()
    }

    /// The saved network named `ssid`.
    #[must_use]
    pub fn network(&self, ssid: &str) -> Option<&SavedNetwork> {
        self.networks.iter().find(|network| network.ssid == ssid)
    }
}

/// The fingerprint a [`SavedNetwork`] carries for its password: FNV-1a
/// over the bytes. It only has to change when the password changes; it is
/// kept in RAM and never leaves the board or a log.
#[must_use]
pub fn secret_tag(password: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in password.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn set_to_use_wifi_needs_the_switch_and_a_network() {
        let one = vec![SavedNetwork {
            ssid: String::from("lp-walk-net"),
            hidden: false,
            secret_tag: secret_tag("correct-horse-42"),
        }];
        assert!(
            StationSettings {
                wifi: true,
                networks: one.clone()
            }
            .uses_wifi()
        );
        assert!(
            !StationSettings {
                wifi: false,
                networks: one
            }
            .uses_wifi()
        );
        assert!(
            !StationSettings {
                wifi: true,
                networks: Vec::new()
            }
            .uses_wifi()
        );
    }

    #[test]
    fn the_tag_follows_the_password() {
        assert_eq!(
            secret_tag("correct-horse-42"),
            secret_tag("correct-horse-42")
        );
        assert_ne!(
            secret_tag("correct-horse-42"),
            secret_tag("correct-horse-43")
        );
    }
}
