//! Reading and changing the device network file through the server's OWN
//! filesystem.
//!
//! The network file (`/.lp/network.json`, [`NetworkFile`]) holds the Wi-Fi
//! passwords, so the wire can never read it (the fs gate refuses, on every
//! link, as it does for the access files). The server reads it here,
//! directly off its base fs, and answers the edit-tier network requests
//! (`NetworkStatus`, `NetworkAdd`, `NetworkForget`, `NetworkSet`) here too
//! — a read-modify-write that never goes through the wire fs path, and
//! whose answer ([`NetworkStatus`]) never carries a password.
//!
//! A **missing** file is [`NetworkFile::none`] (no network, Wi-Fi and the
//! relay on). A **damaged** one reads as no network too, logged without its
//! bytes, and is left as it is until the next change replaces it.
//!
//! Nothing here joins a network or listens for one: what the station is
//! doing and what it hears come from the embedder's probes
//! ([`crate::StationProbe`], [`crate::ScanProbe`]), and no M5 image
//! installs either, so every board answers [`StationState::Unsupported`]
//! and a scan [`NetworkScan::Unsupported`].

extern crate alloc;

use alloc::format;
use alloc::string::String;
use lpc_access::{NetworkFile, WifiNetwork};
use lpc_model::AsLpPath;
use lpc_wire::WifiPassword;
use lpc_wire::server::{NetworkScan, NetworkStatus, ServerMsgBody, StationState};
use lpfs::LpFs;

/// Why a change is refused on a board holding its files for the C6 layout
/// change: it runs on a RAM filesystem, and a write there would vanish.
pub const HELD_BOARD_REFUSAL: &str =
    "the board is holding its files for an update; finish the update first";

/// The network file at root `/.lp/network.json`: [`NetworkFile::none`] when
/// there is none, and when it cannot be read (logged, never its bytes).
///
/// Missing is decided by `file_exists`, as the device store's is.
pub fn read_network_file(fs: &dyn LpFs) -> NetworkFile {
    let path = NetworkFile::PATH.as_path();
    match fs.file_exists(path) {
        Ok(false) => NetworkFile::none(),
        Ok(true) => match fs.read_file(path) {
            Ok(bytes) => NetworkFile::from_json(&bytes).unwrap_or_else(|error| unreadable(&error)),
            Err(error) => unreadable(&error),
        },
        Err(error) => unreadable(&error),
    }
}

/// The one place an unreadable network file is logged. Neither error
/// spells out the file's bytes: an fs error names the path, and
/// `NetworkFileError` names a position or a rule.
#[inline(never)]
fn unreadable(error: &dyn core::fmt::Display) -> NetworkFile {
    log::warn!("network: network file unreadable, treating as no network: {error}");
    NetworkFile::none()
}

/// Write the network file, always at the current version. `Err` is the
/// failure in words (the fs's own message, or the serializer's position),
/// never the file's bytes.
pub fn write_network_file(fs: &dyn LpFs, file: &NetworkFile) -> Result<(), String> {
    match file.to_json() {
        Ok(json) => fs
            .write_file(NetworkFile::PATH.as_path(), json.as_bytes())
            .map_err(|error| write_failed(&error)),
        Err(error) => Err(write_failed(&error)),
    }
}

/// The one place a failed write is put in words.
#[inline(never)]
fn write_failed(error: &dyn core::fmt::Display) -> String {
    format!("cannot save the network settings: {error}")
}

/// `NetworkStatus`: the two switches, every saved network without its
/// password, and `station`.
#[inline(never)]
pub fn network_status(fs: &dyn LpFs, station: StationState) -> ServerMsgBody {
    status_body(&read_network_file(fs), station)
}

/// `NetworkScan`: what the radio heard, or `unsupported` — the probe's
/// answer, passed through.
#[inline(never)]
pub fn network_scan(scan: NetworkScan) -> ServerMsgBody {
    ServerMsgBody::NetworkScan(scan)
}

/// `NetworkAdd`: save `ssid` with `password` and answer the status. A new
/// name goes last; a saved one keeps its place and takes the new password
/// (and `hidden`, when given). A ninth network, or one that breaks the
/// 802.11 / WPA2 rules, answers [`ServerMsgBody::Error`] with the rule's
/// code (never the password) and writes nothing — Studio's own early
/// validation, run before a request is ever sent, is what the user
/// actually sees in words; this reply is the rare fallback (e.g. `lp-cli`,
/// which sends no early check of its own).
#[inline(never)]
pub fn network_add(
    fs: &dyn LpFs,
    station: StationState,
    ssid: String,
    password: WifiPassword,
    hidden: Option<bool>,
) -> ServerMsgBody {
    let mut file = read_network_file(fs);
    let hidden = hidden.unwrap_or_else(|| file.network(&ssid).is_some_and(|saved| saved.hidden));
    if let Err(rule) = file.add(WifiNetwork {
        ssid,
        password: password.into_inner(),
        hidden,
    }) {
        return ServerMsgBody::Error {
            error: format!("cannot save the network: {rule}"),
        };
    }
    save_and_answer(fs, &file, true, station)
}

/// `NetworkForget`: drop the saved network named `ssid` (name and
/// password) and answer the status. Forgetting one that is not saved is
/// not an error, and writes nothing.
#[inline(never)]
pub fn network_forget(fs: &dyn LpFs, station: StationState, ssid: &str) -> ServerMsgBody {
    let mut file = read_network_file(fs);
    let forgot = file.forget(ssid);
    save_and_answer(fs, &file, forgot, station)
}

/// `NetworkSet`: change whichever switch is given and answer the status.
/// A switch already where it is asked to be writes nothing.
#[inline(never)]
pub fn network_set(
    fs: &dyn LpFs,
    station: StationState,
    wifi: Option<bool>,
    cloud_relay: Option<bool>,
) -> ServerMsgBody {
    let mut file = read_network_file(fs);
    let changed = wifi.is_some_and(|on| on != file.wifi)
        || cloud_relay.is_some_and(|on| on != file.cloud_relay);
    file.wifi = wifi.unwrap_or(file.wifi);
    file.cloud_relay = cloud_relay.unwrap_or(file.cloud_relay);
    save_and_answer(fs, &file, changed, station)
}

/// Write `file` when `changed`, then answer its status. A failed write
/// answers an error (the fs's own message, which holds no password).
fn save_and_answer(
    fs: &dyn LpFs,
    file: &NetworkFile,
    changed: bool,
    station: StationState,
) -> ServerMsgBody {
    if changed {
        if let Err(message) = write_network_file(fs, file) {
            return ServerMsgBody::Error { error: message };
        }
    }
    status_body(file, station)
}

fn status_body(file: &NetworkFile, station: StationState) -> ServerMsgBody {
    ServerMsgBody::NetworkStatus(NetworkStatus::of(file, station))
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
    fn a_short_password_writes_nothing() {
        let fs = LpFsMemory::new();
        let body = network_add(
            &fs,
            StationState::Unsupported,
            String::from("lp-walk-net"),
            WifiPassword::new("short"),
            None,
        );
        assert!(
            matches!(&body, ServerMsgBody::Error { error } if error.contains("passwordTooShort")),
            "{body:?}"
        );
        assert!(!fs.file_exists(NetworkFile::PATH.as_path()).unwrap());
    }

    #[test]
    fn adding_again_keeps_hidden_unless_told() {
        let fs = LpFsMemory::new();
        let station = || StationState::Unsupported;
        network_add(
            &fs,
            station(),
            String::from("lp-back-office"),
            WifiPassword::new(""),
            Some(true),
        );
        network_add(
            &fs,
            station(),
            String::from("lp-back-office"),
            WifiPassword::new("staple-battery-7"),
            None,
        );
        let file = read_network_file(&fs);
        assert!(file.networks[0].hidden, "an absent hidden keeps it");
        assert_eq!(file.networks[0].password, "staple-battery-7");
        network_add(
            &fs,
            station(),
            String::from("lp-back-office"),
            WifiPassword::new("staple-battery-7"),
            Some(false),
        );
        assert!(!read_network_file(&fs).networks[0].hidden);
    }
}
