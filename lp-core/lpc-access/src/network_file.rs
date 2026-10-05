//! The device network file: root `/.lp/network.json`.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::network_file_error::NetworkFileError;
use crate::wifi_network::WifiNetwork;

/// The device's network settings, at root `/.lp/network.json` — a sibling
/// of the device store (`/.lp/access.json`) with its own `version`.
///
/// ```json
/// {"version":1,"wifi":true,"cloudRelay":true,
///  "networks":[{"ssid":"lp-walk-net","password":"…"},{"ssid":"back-office","password":"…","hidden":true}]}
/// ```
///
/// - `wifi` — the board's one Wi-Fi switch. On by default, so a missing
///   field reads as on.
/// - `cloudRelay` — lets lightplayer.app reach this board through the
///   cloud relay. On by default, so a missing field reads as on.
/// - `networks` — the saved networks ([`WifiNetwork`]), in the order they
///   were added, at most [`Self::MAX_NETWORKS`], no two with one name.
///   Adding a name that is already saved changes its password in place
///   ([`Self::add`]). Which one the station joins is the station's call
///   (M6): the strongest saved network it hears, skipping one whose
///   password was refused — there is no priority order.
///
/// **Write-only on every link.** The passwords are secrets: no link at any
/// tier reads this file ([`crate::is_write_only_file_path`]); the server
/// reads it through its own filesystem and answers a status that carries
/// each network's name and whether it has a password, never the password.
/// `Debug` never prints one either.
///
/// A missing file is [`Self::none`] (no network, Wi-Fi and the relay on). A
/// file that fails to parse reads as no network too (the caller logs it)
/// and is not rewritten until the next change.
///
/// **Format bumps.** A serde change to this type or to [`WifiNetwork`] is a
/// format change, even one that adds or removes no field: bump
/// [`Self::VERSION`], keep a private reader for every older version that
/// converts on read (as [`crate::DeviceAccessFile`] keeps v1 and v2), pin
/// the old bytes as a `const` in a test, and regenerate
/// `schemas/device-network.schema.json`. The device reads old versions and
/// writes the current one; `lpa-upgrade` (projects) is not involved, and
/// the file is outside `PROJECT_FORMAT_VERSION`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkFile {
    /// Format version; always [`NetworkFile::VERSION`] when written.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1, max = 1)))]
    pub version: u32,
    /// The board's Wi-Fi switch. On by default: a missing field reads as on.
    #[serde(default = "on")]
    pub wifi: bool,
    /// Lets lightplayer.app reach this board through the cloud relay. On by
    /// default: a missing field reads as on.
    #[serde(default = "on")]
    pub cloud_relay: bool,
    /// The saved networks, in the order they were added (at most 8, no two
    /// with one name).
    #[serde(default)]
    #[cfg_attr(feature = "schema-gen", schemars(length(max = 8)))]
    pub networks: Vec<WifiNetwork>,
}

impl NetworkFile {
    /// The format version this build writes.
    pub const VERSION: u32 = 1;

    /// Absolute path of the network file on the device filesystem.
    pub const PATH: &'static str = "/.lp/network.json";

    /// The most networks a board keeps.
    pub const MAX_NETWORKS: usize = 8;

    /// The state a device without a network file is in: no network saved,
    /// Wi-Fi and the relay on.
    #[must_use]
    pub fn none() -> Self {
        Self {
            version: Self::VERSION,
            wifi: true,
            cloud_relay: true,
            networks: Vec::new(),
        }
    }

    /// The saved network named `ssid`, if there is one.
    #[must_use]
    pub fn network(&self, ssid: &str) -> Option<&WifiNetwork> {
        self.networks.iter().find(|network| network.ssid == ssid)
    }

    /// Save `network`, after checking it against the 802.11 / WPA2 rules.
    /// A name already saved keeps its place and takes the new password (and
    /// `hidden`); a new one goes last, unless [`Self::MAX_NETWORKS`] are
    /// already saved.
    pub fn add(&mut self, network: WifiNetwork) -> Result<(), NetworkFileError> {
        network.validate()?;
        if let Some(saved) = self
            .networks
            .iter_mut()
            .find(|saved| saved.ssid == network.ssid)
        {
            *saved = network;
            return Ok(());
        }
        if self.networks.len() >= Self::MAX_NETWORKS {
            return Err(NetworkFileError::TooManyNetworks {
                max: Self::MAX_NETWORKS,
            });
        }
        self.networks.push(network);
        Ok(())
    }

    /// Forget the network named `ssid` (its name and password). Whether one
    /// was saved.
    pub fn forget(&mut self, ssid: &str) -> bool {
        let before = self.networks.len();
        self.networks.retain(|network| network.ssid != ssid);
        self.networks.len() != before
    }

    /// Parse and validate the file's bytes. The version is checked first,
    /// so a newer file is refused by its number, not by a field it adds.
    pub fn from_json(bytes: &[u8]) -> Result<Self, NetworkFileError> {
        let version = crate::secret_entry::read_version_field(bytes).map_err(malformed)?;
        if version != Self::VERSION {
            return Err(NetworkFileError::UnsupportedVersion(version));
        }
        let file = serde_json::from_slice::<Self>(bytes).map_err(malformed)?;
        file.validate()?;
        Ok(file)
    }

    /// Serialize to the file's bytes, always at [`Self::VERSION`].
    pub fn to_json(&self) -> Result<String, NetworkFileError> {
        let current = Self {
            version: Self::VERSION,
            ..self.clone()
        };
        serde_json::to_string(&current).map_err(malformed)
    }

    /// Every network meets the rules, there are at most
    /// [`Self::MAX_NETWORKS`], and no two share a name.
    fn validate(&self) -> Result<(), NetworkFileError> {
        if self.networks.len() > Self::MAX_NETWORKS {
            return Err(NetworkFileError::TooManyNetworks {
                max: Self::MAX_NETWORKS,
            });
        }
        for (at, network) in self.networks.iter().enumerate() {
            network.validate()?;
            if self.networks[..at]
                .iter()
                .any(|earlier| earlier.ssid == network.ssid)
            {
                return Err(NetworkFileError::DuplicateSsid);
            }
        }
        Ok(())
    }
}

impl Default for NetworkFile {
    /// [`Self::none`]: no network, Wi-Fi and the relay on.
    fn default() -> Self {
        Self::none()
    }
}

/// A switch that is on unless the file turns it off.
fn on() -> bool {
    true
}

/// Where the parse stopped — never serde's message, which may quote a value.
fn malformed(error: serde_json::Error) -> NetworkFileError {
    NetworkFileError::Malformed {
        line: error.line(),
        column: error.column(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;

    /// The version-1 file exactly as this writer produces it.
    const V1_FILE: &str = "{\"version\":1,\"wifi\":true,\"cloudRelay\":true,\"networks\":[\
        {\"ssid\":\"lp-walk-net\",\"password\":\"correct-horse-42\"},\
        {\"ssid\":\"lp-back-office\",\"password\":\"staple-battery-7\",\"hidden\":true}]}";

    fn network(ssid: &str, password: &str) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.to_string(),
            password: password.to_string(),
            hidden: false,
        }
    }

    fn saved() -> NetworkFile {
        NetworkFile {
            version: NetworkFile::VERSION,
            wifi: true,
            cloud_relay: true,
            networks: alloc::vec![
                network("lp-walk-net", "correct-horse-42"),
                WifiNetwork {
                    hidden: true,
                    ..network("lp-back-office", "staple-battery-7")
                },
            ],
        }
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let json = saved().to_json().unwrap();
        assert_eq!(json, V1_FILE);
        assert_eq!(NetworkFile::from_json(V1_FILE.as_bytes()).unwrap(), saved());
    }

    #[test]
    fn absent_fields_read_as_none() {
        let file = NetworkFile::from_json(b"{\"version\":1}").unwrap();
        assert_eq!(file, NetworkFile::none());
        assert!(file.wifi, "a missing wifi means Wi-Fi is on");
        assert!(
            file.cloud_relay,
            "a missing cloudRelay means the relay is on"
        );
        let off =
            NetworkFile::from_json(b"{\"version\":1,\"wifi\":false,\"cloudRelay\":false}").unwrap();
        assert!(!off.wifi);
        assert!(!off.cloud_relay);
        assert!(off.networks.is_empty());
    }

    #[test]
    fn none_writes_an_empty_list() {
        assert_eq!(
            NetworkFile::none().to_json().unwrap(),
            "{\"version\":1,\"wifi\":true,\"cloudRelay\":true,\"networks\":[]}"
        );
        assert_eq!(NetworkFile::default(), NetworkFile::none());
    }

    #[test]
    fn adding_a_saved_name_changes_its_password_in_place() {
        let mut file = saved();
        file.add(network("lp-walk-net", "new-horse-4242")).unwrap();
        assert_eq!(file.networks.len(), 2);
        assert_eq!(file.networks[0].ssid, "lp-walk-net", "it keeps its place");
        assert_eq!(file.networks[0].password, "new-horse-4242");
        file.add(network("lp-third", "")).unwrap();
        assert_eq!(file.networks[2].ssid, "lp-third", "a new one goes last");
    }

    #[test]
    fn a_ninth_network_is_refused() {
        let mut file = NetworkFile::none();
        for n in 0..NetworkFile::MAX_NETWORKS {
            file.add(network(&format!("net-{n}"), "")).unwrap();
        }
        assert_eq!(
            file.add(network("net-9", "")),
            Err(NetworkFileError::TooManyNetworks { max: 8 })
        );
        // A saved name still changes with eight saved.
        file.add(network("net-3", "a-new-password")).unwrap();
        assert_eq!(file.networks.len(), 8);
    }

    #[test]
    fn add_checks_the_rules_and_writes_nothing_when_they_fail() {
        let mut file = saved();
        assert_eq!(
            file.add(network("lp-walk-net", "short")),
            Err(NetworkFileError::PasswordTooShort { len: 5 })
        );
        assert_eq!(file, saved());
    }

    #[test]
    fn forget_drops_one_and_keeps_the_rest_in_order() {
        let mut file = saved();
        file.add(network("lp-third", "")).unwrap();
        assert!(file.forget("lp-back-office"));
        assert!(!file.forget("lp-back-office"), "already gone");
        let names: Vec<&str> = file.networks.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, ["lp-walk-net", "lp-third"]);
        assert!(file.network("lp-third").is_some());
    }

    #[test]
    fn a_newer_version_is_refused_by_its_number() {
        assert_eq!(
            NetworkFile::from_json(b"{\"version\":2,\"somethingNew\":1}"),
            Err(NetworkFileError::UnsupportedVersion(2))
        );
        assert_eq!(
            NetworkFile::from_json(b"{\"version\":0}"),
            Err(NetworkFileError::UnsupportedVersion(0))
        );
    }

    #[test]
    fn an_unknown_field_is_refused() {
        assert!(matches!(
            NetworkFile::from_json(b"{\"version\":1,\"relay\":true}"),
            Err(NetworkFileError::Malformed { .. })
        ));
        assert!(matches!(
            NetworkFile::from_json(
                b"{\"version\":1,\"networks\":[{\"ssid\":\"a\",\"password\":\"\",\"enabled\":true}]}"
            ),
            Err(NetworkFileError::Malformed { .. })
        ));
    }

    #[test]
    fn garbage_is_malformed_without_quoting_it() {
        let bytes = b"{\"version\":1,\"networks\":[{\"ssid\":\"a\",\"password\":\"correct-horse-42\",\"hidden\":\"correct-horse-42\"}]}";
        let error = NetworkFile::from_json(bytes).unwrap_err();
        assert!(matches!(error, NetworkFileError::Malformed { .. }));
        assert!(!format!("{error} {error:?}").contains("correct-horse-42"));
        assert!(matches!(
            NetworkFile::from_json(b"not json"),
            Err(NetworkFileError::Malformed { .. })
        ));
    }

    #[test]
    fn a_stored_list_that_breaks_the_rules_is_refused() {
        let bytes = b"{\"version\":1,\"networks\":[{\"ssid\":\"\",\"password\":\"\"}]}";
        assert_eq!(
            NetworkFile::from_json(bytes),
            Err(NetworkFileError::SsidEmpty)
        );
        let bytes = b"{\"version\":1,\"networks\":[{\"ssid\":\"a\",\"password\":\"short\"}]}";
        assert_eq!(
            NetworkFile::from_json(bytes),
            Err(NetworkFileError::PasswordTooShort { len: 5 })
        );
        let bytes = b"{\"version\":1,\"networks\":[{\"ssid\":\"a\",\"password\":\"\"},{\"ssid\":\"a\",\"password\":\"\"}]}";
        assert_eq!(
            NetworkFile::from_json(bytes),
            Err(NetworkFileError::DuplicateSsid)
        );
        let nine: Vec<String> = (0..9)
            .map(|n| format!("{{\"ssid\":\"n{n}\",\"password\":\"\"}}"))
            .collect();
        let bytes = format!("{{\"version\":1,\"networks\":[{}]}}", nine.join(","));
        assert_eq!(
            NetworkFile::from_json(bytes.as_bytes()),
            Err(NetworkFileError::TooManyNetworks { max: 8 })
        );
    }

    #[test]
    fn to_json_always_writes_the_current_version() {
        let mut file = saved();
        file.version = 7;
        assert!(file.to_json().unwrap().starts_with("{\"version\":1,"));
    }

    #[test]
    fn debug_never_prints_a_password() {
        for shown in [format!("{:?}", saved()), format!("{:#?}", saved())] {
            assert!(!shown.contains("correct-horse-42"), "{shown}");
            assert!(!shown.contains("staple-battery-7"), "{shown}");
            assert!(shown.contains("lp-walk-net"), "{shown}");
        }
    }
}
