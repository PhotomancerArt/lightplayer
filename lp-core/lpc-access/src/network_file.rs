//! The device network file: root `/.lp/network.json`.

use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::network_file_error::NetworkFileError;
use crate::wifi_network::WifiNetwork;

/// The device's network settings, at root `/.lp/network.json` — a sibling
/// of the device store (`/.lp/access.json`) with its own `version`.
///
/// ```json
/// {"version":1,"wifi":{"ssid":"lp-walk-net","password":"…","enabled":true},"cloudRelay":true}
/// ```
///
/// - `wifi` — the saved network ([`WifiNetwork`]); absent when none is.
/// - `cloudRelay` — lets lightplayer.app reach this board through the
///   cloud relay. On by default, so a missing field reads as on.
///
/// **Write-only on every link.** The password is a secret: no link at any
/// tier reads this file ([`crate::is_write_only_file_path`]); the server
/// reads it through its own filesystem and answers a status that carries
/// the network name and whether a password is set, never the password.
/// `Debug` never prints it either.
///
/// A missing file is [`Self::none`] (no network, relay allowed). A file
/// that fails to parse reads as no network too (the caller logs it) and is
/// not rewritten until the next change.
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
    /// The saved Wi-Fi network; absent when none is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wifi: Option<WifiNetwork>,
    /// Lets lightplayer.app reach this board through the cloud relay. On by
    /// default: a missing field reads as on.
    #[serde(default = "cloud_relay_default")]
    pub cloud_relay: bool,
}

impl NetworkFile {
    /// The format version this build writes.
    pub const VERSION: u32 = 1;

    /// Absolute path of the network file on the device filesystem.
    pub const PATH: &'static str = "/.lp/network.json";

    /// The state a device without a network file is in: no network saved,
    /// the relay allowed.
    #[must_use]
    pub fn none() -> Self {
        Self {
            version: Self::VERSION,
            wifi: None,
            cloud_relay: true,
        }
    }

    /// Parse and validate the file's bytes. The version is checked first,
    /// so a newer file is refused by its number, not by a field it adds.
    pub fn from_json(bytes: &[u8]) -> Result<Self, NetworkFileError> {
        let version = crate::secret_entry::read_version_field(bytes).map_err(malformed)?;
        if version != Self::VERSION {
            return Err(NetworkFileError::UnsupportedVersion(version));
        }
        let file = serde_json::from_slice::<Self>(bytes).map_err(malformed)?;
        if let Some(wifi) = &file.wifi {
            wifi.validate()?;
        }
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
}

impl Default for NetworkFile {
    /// [`Self::none`]: no network, the relay allowed.
    fn default() -> Self {
        Self::none()
    }
}

/// The cloud relay is on unless the file turns it off.
fn cloud_relay_default() -> bool {
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
    const V1_FILE: &str = "{\"version\":1,\"wifi\":{\"ssid\":\"lp-walk-net\",\
        \"password\":\"correct-horse-42\",\"enabled\":true},\"cloudRelay\":true}";

    fn saved() -> NetworkFile {
        NetworkFile {
            version: NetworkFile::VERSION,
            wifi: Some(WifiNetwork {
                ssid: "lp-walk-net".to_string(),
                password: "correct-horse-42".to_string(),
                enabled: true,
            }),
            cloud_relay: true,
        }
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let json = saved().to_json().unwrap();
        assert_eq!(json, V1_FILE);
        assert_eq!(NetworkFile::from_json(V1_FILE.as_bytes()).unwrap(), saved());
    }

    #[test]
    fn absent_wifi_and_absent_cloud_relay_read_as_none() {
        let file = NetworkFile::from_json(b"{\"version\":1}").unwrap();
        assert_eq!(file, NetworkFile::none());
        assert!(
            file.cloud_relay,
            "a missing cloudRelay means the relay is on"
        );
        let off = NetworkFile::from_json(b"{\"version\":1,\"cloudRelay\":false}").unwrap();
        assert!(!off.cloud_relay);
        assert!(off.wifi.is_none());
    }

    #[test]
    fn none_writes_no_wifi_key() {
        assert_eq!(
            NetworkFile::none().to_json().unwrap(),
            "{\"version\":1,\"cloudRelay\":true}"
        );
        assert_eq!(NetworkFile::default(), NetworkFile::none());
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
                b"{\"version\":1,\"wifi\":{\"ssid\":\"a\",\"password\":\"\",\"enabled\":true,\"x\":1}}"
            ),
            Err(NetworkFileError::Malformed { .. })
        ));
    }

    #[test]
    fn garbage_is_malformed_without_quoting_it() {
        let bytes = b"{\"version\":1,\"wifi\":{\"ssid\":\"a\",\"password\":\"correct-horse-42\",\"enabled\":\"correct-horse-42\"}}";
        let error = NetworkFile::from_json(bytes).unwrap_err();
        assert!(matches!(error, NetworkFileError::Malformed { .. }));
        assert!(!format!("{error} {error:?}").contains("correct-horse-42"));
        assert!(matches!(
            NetworkFile::from_json(b"not json"),
            Err(NetworkFileError::Malformed { .. })
        ));
    }

    #[test]
    fn a_stored_network_that_breaks_the_rules_is_refused() {
        let bytes = b"{\"version\":1,\"wifi\":{\"ssid\":\"\",\"password\":\"\",\"enabled\":true}}";
        assert_eq!(
            NetworkFile::from_json(bytes),
            Err(NetworkFileError::SsidEmpty)
        );
        let bytes =
            b"{\"version\":1,\"wifi\":{\"ssid\":\"a\",\"password\":\"short\",\"enabled\":true}}";
        assert_eq!(
            NetworkFile::from_json(bytes),
            Err(NetworkFileError::PasswordTooShort { len: 5 })
        );
    }

    #[test]
    fn to_json_always_writes_the_current_version() {
        let mut file = saved();
        file.version = 7;
        assert!(file.to_json().unwrap().starts_with("{\"version\":1,"));
    }

    #[test]
    fn debug_never_prints_the_password() {
        let shown = format!("{:?}", saved());
        assert!(!shown.contains("correct-horse-42"), "{shown}");
        assert!(shown.contains("lp-walk-net"), "{shown}");
        let pretty = format!("{:#?}", saved());
        assert!(!pretty.contains("correct-horse-42"), "{pretty}");
    }
}
