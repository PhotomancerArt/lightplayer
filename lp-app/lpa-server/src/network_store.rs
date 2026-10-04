//! Reading and changing the device network file through the server's OWN
//! filesystem.
//!
//! The network file (`/.lp/network.json`, [`NetworkFile`]) holds the Wi-Fi
//! password, so the wire can never read it (the fs gate refuses, on every
//! link, as it does for the access files). The server reads it here,
//! directly off its base fs, and answers the edit-tier network requests
//! (`NetworkStatus`, `NetworkSet`, `NetworkForget`) here too — a
//! read-modify-write that never goes through the wire fs path, and whose
//! answer ([`NetworkStatus`]) never carries the password.
//!
//! A **missing** file is [`NetworkFile::none`] (no network, relay allowed).
//! A **damaged** one reads as no network too, logged without its bytes, and
//! is left as it is until the next change replaces it.
//!
//! Nothing here joins a network: what the station is doing comes from the
//! embedder's probe ([`crate::StationProbe`]), and no M5 image installs one,
//! so every board answers [`StationState::Unsupported`].

extern crate alloc;

use alloc::format;
use alloc::string::String;
use lpc_access::{NetworkFile, WifiNetwork, validate_password, validate_ssid};
use lpc_model::AsLpPath;
use lpc_wire::WifiPassword;
use lpc_wire::server::{NetworkStatus, ServerMsgBody, StationState};
use lpfs::LpFs;

/// Why a change is refused on a board holding its files for the C6 layout
/// change: it runs on a RAM filesystem, and a write there would vanish.
pub const HELD_BOARD_REFUSAL: &str =
    "the board is holding its files for an update; finish the update first";

/// Why `enabled` or `password` alone is refused with no network saved.
pub const NO_NETWORK_SAVED: &str = "no network is saved; send its name";

/// Why a new network name without a password is refused.
pub const NEW_NETWORK_NEEDS_PASSWORD: &str =
    "a new network needs its password (empty for an open network)";

/// The network file at root `/.lp/network.json`: [`NetworkFile::none`] when
/// there is none, and when it cannot be read (logged, never its bytes).
///
/// Missing is decided by `file_exists`, as the device store's is.
pub fn read_network_file(fs: &dyn LpFs) -> NetworkFile {
    let path = NetworkFile::PATH.as_path();
    let read = match fs.file_exists(path) {
        Ok(false) => return NetworkFile::none(),
        Ok(true) => fs
            .read_file(path)
            .map_err(|error| format!("{error}"))
            .and_then(|bytes| NetworkFile::from_json(&bytes).map_err(|error| format!("{error}"))),
        Err(error) => Err(format!("{error}")),
    };
    // Neither error spells out the file's bytes: an fs error names the
    // path, and `NetworkFileError` names a position or a rule.
    read.unwrap_or_else(|error| {
        log::warn!("network: network file unreadable, treating as no network: {error}");
        NetworkFile::none()
    })
}

/// Write the network file, always at the current version.
pub fn write_network_file(fs: &dyn LpFs, file: &NetworkFile) -> Result<(), String> {
    let json = file.to_json().map_err(|error| format!("{error}"))?;
    fs.write_file(NetworkFile::PATH.as_path(), json.as_bytes())
        .map_err(|error| format!("{error}"))
}

/// `NetworkStatus`: the saved network without its password, `cloudRelay`, and
/// `station`.
#[inline(never)]
pub fn network_status(fs: &dyn LpFs, station: StationState) -> ServerMsgBody {
    status_body(&read_network_file(fs), station)
}

/// `NetworkSet`: change whichever settings are given, write the file when
/// something changed, and answer the status.
///
/// - A `ssid` different from the saved one (or with none saved) needs
///   `password` too, so an old password is never offered to a new network.
/// - `enabled` or `password` with no network saved and no `ssid` is refused.
/// - A network saved for the first time is switched on unless `enabled`
///   says otherwise.
/// - `cloud_relay` may be set with no network saved.
///
/// A broken rule answers [`ServerMsgBody::Error`] with a sentence that names
/// the rule (never the password) and writes nothing.
#[inline(never)]
pub fn network_set(
    fs: &dyn LpFs,
    station: StationState,
    ssid: Option<String>,
    password: Option<WifiPassword>,
    enabled: Option<bool>,
    cloud_relay: Option<bool>,
) -> ServerMsgBody {
    let current = read_network_file(fs);
    let mut next = current.clone();

    match (next.wifi.take(), ssid) {
        // A network named for the first time, or renamed: it needs its
        // password, and starts switched on.
        (saved, Some(ssid)) if saved.as_ref().is_none_or(|saved| saved.ssid != ssid) => {
            let Some(password) = password else {
                return error(NEW_NETWORK_NEEDS_PASSWORD);
            };
            next.wifi = Some(WifiNetwork {
                ssid,
                password: password.into_inner(),
                enabled: enabled.unwrap_or(true),
            });
        }
        // The saved network (named again, or not named): change what is given.
        (Some(mut saved), _) => {
            if let Some(password) = password {
                saved.password = password.into_inner();
            }
            if let Some(enabled) = enabled {
                saved.enabled = enabled;
            }
            next.wifi = Some(saved);
        }
        // Nothing saved and no name: only `cloudRelay` can change.
        (None, _) => {
            if password.is_some() || enabled.is_some() {
                return error(NO_NETWORK_SAVED);
            }
        }
    }
    if let Some(cloud_relay) = cloud_relay {
        next.cloud_relay = cloud_relay;
    }
    if let Some(wifi) = &next.wifi {
        if let Err(rule) =
            validate_ssid(&wifi.ssid).and_then(|()| validate_password(&wifi.password))
        {
            return error(&format!("cannot save the network: {rule}"));
        }
    }
    write_if_changed(fs, &current, &next, station)
}

/// `NetworkForget`: drop the saved network (name and password), keep
/// `cloudRelay`, and answer the status. Forgetting with nothing saved is not
/// an error, and writes nothing.
#[inline(never)]
pub fn network_forget(fs: &dyn LpFs, station: StationState) -> ServerMsgBody {
    let current = read_network_file(fs);
    let next = NetworkFile {
        wifi: None,
        ..current.clone()
    };
    write_if_changed(fs, &current, &next, station)
}

/// Write `next` when it differs from `current`, then answer its status. A
/// failed write answers an error (the fs's own message, which holds no
/// password).
fn write_if_changed(
    fs: &dyn LpFs,
    current: &NetworkFile,
    next: &NetworkFile,
    station: StationState,
) -> ServerMsgBody {
    if next != current {
        if let Err(message) = write_network_file(fs, next) {
            return error(&format!("cannot save the network settings: {message}"));
        }
    }
    status_body(next, station)
}

fn status_body(file: &NetworkFile, station: StationState) -> ServerMsgBody {
    ServerMsgBody::NetworkStatus(NetworkStatus::of(file, station))
}

fn error(message: &str) -> ServerMsgBody {
    ServerMsgBody::Error {
        error: String::from(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    #[test]
    fn a_missing_file_is_no_network() {
        let fs = LpFsMemory::new();
        assert_eq!(read_network_file(&fs), NetworkFile::none());
    }

    #[test]
    fn a_damaged_file_is_no_network_and_is_left_alone() {
        let fs = LpFsMemory::new();
        fs.write_file(NetworkFile::PATH.as_path(), b"{not json")
            .unwrap();
        assert_eq!(read_network_file(&fs), NetworkFile::none());
        let _ = network_status(&fs, StationState::Unsupported);
        assert_eq!(
            fs.read_file(NetworkFile::PATH.as_path()).unwrap(),
            b"{not json"
        );
    }

    #[test]
    fn a_rename_without_a_password_writes_nothing() {
        let fs = LpFsMemory::new();
        let body = network_set(
            &fs,
            StationState::Unsupported,
            Some(String::from("lp-walk-net")),
            None,
            None,
            None,
        );
        assert!(
            matches!(&body, ServerMsgBody::Error { error } if error == NEW_NETWORK_NEEDS_PASSWORD),
            "{body:?}"
        );
        assert!(!fs.file_exists(NetworkFile::PATH.as_path()).unwrap());
    }
}
