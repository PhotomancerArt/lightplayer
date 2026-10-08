//! [`UiDeviceWifi`]: what the device card's Wi‑Fi row and its popover read.
//!
//! The popover has three pages (the spike's 2B): **Networks** — the
//! connected one first, then the other saved ones, then "+ Connect to a
//! network"; **Connect to a network** — what the board hears, strongest
//! first, and "Other network…"; and **the network's page** — its name and
//! password, then Connect. Which page shows is the popover's own state;
//! everything each page says comes from here.

use lpa_devices::DeviceId;
use lpc_wire::server::{
    HeardNetwork, LastAttempt, NetworkStatus, SavedNetworkInfo, StationFailure, StationState,
};

use super::ui_wifi_test::UiWifiTest;
use super::wifi_network_slug::wifi_network_slugs;
use super::wifi_words::{self, WifiTone, page, row};

/// One device's Wi‑Fi facts, joined onto its card (the Connections
/// group's Wi‑Fi row). Present for every LightPlayer board on a link that
/// has unlocked; the controls themselves are offers at
/// `devices/<board>/wifi/…`, published only while the link holds edit.
///
/// It carries no password: Studio never has one to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDeviceWifi {
    pub device: DeviceId,
    /// The link holds edit (author): the board is asked, and the offers are
    /// published. `false`: the row says [`NEEDS_AUTHOR`] and nothing is
    /// asked.
    pub can_edit: bool,
    /// The board's last answer; `None` until it has answered one.
    pub status: Option<NetworkStatus>,
    /// A read is in flight.
    pub reading: bool,
    /// A change is in flight.
    pub writing: bool,
    /// The last change's (or read's) failure, in the board's words.
    pub error: Option<String>,
    /// What the board's radio heard at the last scan; `None` before one
    /// (and always on a firmware that cannot scan).
    pub heard: Option<Vec<HeardNetwork>>,
    /// A scan is in flight.
    pub scanning: bool,
    /// The network just added, whose test runs in its row until it is
    /// dismissed.
    pub testing: Option<String>,
}

/// One saved network's row on the Networks page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWifiNetworkRow {
    pub ssid: String,
    /// Its id in the forget offer's path (`wifi/forget/<slug>`).
    pub slug: String,
    /// What the board says about it: "Connected · <ip>", "In range", …
    pub word: String,
    pub tone: WifiTone,
    /// The signal, when the board hears it (drawn as bars).
    pub rssi: Option<i8>,
    /// The board is connected to it.
    pub in_use: bool,
    /// `false`: an open network.
    pub has_password: bool,
}

impl UiDeviceWifi {
    /// A device's facts with nothing asked yet.
    pub fn new(device: DeviceId, can_edit: bool) -> Self {
        Self {
            device,
            can_edit,
            status: None,
            reading: false,
            writing: false,
            error: None,
            heard: None,
            scanning: false,
            testing: None,
        }
    }

    /// The card row's value: the connected network, or how it stands.
    pub fn row_value(&self) -> String {
        let Some(status) = &self.status else {
            return if self.can_edit {
                String::new()
            } else {
                "needs author".to_string()
            };
        };
        let saved = status.networks.len();
        match &status.station {
            StationState::Connected { ssid, .. } => ssid.clone(),
            _ if saved == 0 => "set up".to_string(),
            StationState::Unsupported if saved == 1 => {
                format!("{} · saved", status.networks[0].ssid)
            }
            StationState::Unsupported => format!("{saved} saved"),
            StationState::Off => "off".to_string(),
            StationState::Connecting { .. } => "connecting…".to_string(),
            StationState::Failed {
                reason: StationFailure::WrongPassword,
                ..
            } => "wrong password".to_string(),
            StationState::Failed { .. } | StationState::NotConnected => "not connected".to_string(),
        }
    }

    /// How the card row's value reads.
    pub fn row_tone(&self) -> WifiTone {
        match self.status.as_ref().map(|status| &status.station) {
            Some(StationState::Connected { .. }) => WifiTone::Good,
            Some(StationState::Failed {
                reason: StationFailure::WrongPassword,
                ..
            }) => WifiTone::Warn,
            _ => WifiTone::Plain,
        }
    }

    /// The connected network's signal, for the card row's bars.
    pub fn row_rssi(&self) -> Option<i8> {
        match self.status.as_ref().map(|status| &status.station) {
            Some(StationState::Connected { rssi, .. }) => Some(*rssi),
            _ => None,
        }
    }

    /// What the popover says while there is no status: [`READING`] or
    /// [`NEEDS_AUTHOR`].
    pub fn waiting_line(&self) -> Option<&'static str> {
        match (&self.status, self.can_edit) {
            (_, false) => Some(NEEDS_AUTHOR),
            (None, true) => Some(READING),
            (Some(_), true) => None,
        }
    }

    /// Whether the board can scan and connect (its station is not
    /// `unsupported`). `false`: no scan, no test; the connect page is a
    /// typed name and Connect is Save.
    pub fn can_connect(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.station != StationState::Unsupported)
    }

    /// Nothing saved: the popover opens straight on the connect page.
    pub fn nothing_saved(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| status.networks.is_empty())
    }

    /// The board's Wi‑Fi switch (on until the board says otherwise).
    pub fn wifi_on(&self) -> bool {
        self.status.as_ref().is_none_or(|status| status.wifi)
    }

    /// The cloud relay switch (on until the board says otherwise).
    pub fn cloud_relay_on(&self) -> bool {
        self.status.as_ref().is_none_or(|status| status.cloud_relay)
    }

    /// The line under the Cloud relay switch: whether the board reached
    /// lightplayer.app, in the test's words (`wifi_words::relay`).
    pub fn relay_line(&self) -> Option<(&'static str, WifiTone)> {
        wifi_words::relay::line(self.status.as_ref()?)
    }

    /// The Networks page's line under its header, when it has one — none
    /// while a test shows in its row (the row says it).
    pub fn networks_line(&self) -> Option<String> {
        if self.test().is_some() {
            return None;
        }
        wifi_words::networks_page_line(self.status.as_ref()?)
    }

    /// The saved networks as the Networks page lists them: the connected
    /// one first, then the rest in the order they were added.
    pub fn rows(&self) -> Vec<UiWifiNetworkRow> {
        let Some(status) = &self.status else {
            return Vec::new();
        };
        let slugs = wifi_network_slugs(status.networks.iter().map(|n| n.ssid.as_str()));
        let mut rows: Vec<UiWifiNetworkRow> = status
            .networks
            .iter()
            .zip(slugs)
            .map(|(network, slug)| self.row(status, network, slug))
            .collect();
        rows.sort_by_key(|row| !row.in_use);
        rows
    }

    /// The in-row test of the network just added, while it is saved.
    pub fn test(&self) -> Option<UiWifiTest> {
        let ssid = self.testing.as_deref()?;
        let status = self.status.as_ref()?;
        status.network(ssid)?;
        Some(UiWifiTest::of(ssid, status))
    }

    /// What the connect page lists: what the board heard, strongest first,
    /// without the networks already saved.
    pub fn nearby(&self) -> Vec<HeardNetwork> {
        let Some(heard) = &self.heard else {
            return Vec::new();
        };
        let mut nearby: Vec<HeardNetwork> = heard
            .iter()
            .filter(|network| {
                self.status
                    .as_ref()
                    .is_none_or(|status| status.network(&network.ssid).is_none())
            })
            .cloned()
            .collect();
        nearby.sort_by_key(|network| core::cmp::Reverse(network.rssi));
        nearby
    }

    /// Whether the board heard `ssid` as an open network (it needs no
    /// password).
    pub fn heard_open(&self, ssid: &str) -> bool {
        self.heard_as(ssid).is_some_and(|network| !network.secure)
    }

    /// A saved network's page line, and how it reads.
    pub fn network_line(&self, ssid: &str) -> Option<(String, WifiTone)> {
        let status = self.status.as_ref()?;
        let network = status.network(ssid)?;
        Some(match &status.station {
            StationState::Unsupported => (page::SAVED_UNSUPPORTED.to_string(), WifiTone::Plain),
            StationState::Off => (page::SAVED_WIFI_OFF.to_string(), WifiTone::Plain),
            StationState::Connected {
                ssid: on, ip, rssi, ..
            } if on == ssid => (wifi_words::connected_line(*rssi, ip), WifiTone::Good),
            _ if self.refused(status, network) => {
                (page::WRONG_PASSWORD.to_string(), WifiTone::Warn)
            }
            _ => match self.heard_as(ssid) {
                Some(heard) => (page::in_range(heard.rssi), WifiTone::Plain),
                None if self.out_of_range(status, network) => {
                    (page::NOT_IN_RANGE.to_string(), WifiTone::Plain)
                }
                None => (page::SAVED.to_string(), WifiTone::Plain),
            },
        })
    }

    /// A saved network's password line.
    pub fn password_line(&self, ssid: &str) -> Option<&'static str> {
        let network = self.status.as_ref()?.network(ssid)?;
        Some(if network.has_password {
            wifi_words::PASSWORD_SAVED
        } else {
            wifi_words::OPEN_NETWORK
        })
    }

    /// Whether `ssid`'s password was refused (Change password leads).
    pub fn password_refused(&self, ssid: &str) -> bool {
        self.status.as_ref().is_some_and(|status| {
            status
                .network(ssid)
                .is_some_and(|network| self.refused(status, network))
        })
    }

    fn row(
        &self,
        status: &NetworkStatus,
        network: &SavedNetworkInfo,
        slug: String,
    ) -> UiWifiNetworkRow {
        let ssid = network.ssid.as_str();
        let heard = self.heard_as(ssid).map(|heard| heard.rssi);
        let (word, tone, rssi, in_use) = match &status.station {
            StationState::Unsupported | StationState::Off => {
                (row::SAVED.to_string(), WifiTone::Plain, None, false)
            }
            StationState::Connected {
                ssid: on, ip, rssi, ..
            } if on == ssid => (row::connected(ip), WifiTone::Good, Some(*rssi), true),
            StationState::Connecting { ssid: on, .. } if on == ssid => {
                (row::CONNECTING.to_string(), WifiTone::Plain, heard, false)
            }
            _ if self.refused(status, network) => (
                row::WRONG_PASSWORD.to_string(),
                WifiTone::Warn,
                heard,
                false,
            ),
            _ if heard.is_some() => (row::IN_RANGE.to_string(), WifiTone::Plain, heard, false),
            _ if self.out_of_range(status, network) => {
                (row::NOT_IN_RANGE.to_string(), WifiTone::Plain, None, false)
            }
            _ => (row::SAVED.to_string(), WifiTone::Plain, None, false),
        };
        UiWifiNetworkRow {
            ssid: network.ssid.clone(),
            slug,
            word,
            tone,
            rssi,
            in_use,
            has_password: network.has_password,
        }
    }

    fn heard_as(&self, ssid: &str) -> Option<&HeardNetwork> {
        self.heard.as_ref()?.iter().find(|heard| heard.ssid == ssid)
    }

    /// The network refused its password: the station says so now, or its
    /// last attempt did.
    fn refused(&self, status: &NetworkStatus, network: &SavedNetworkInfo) -> bool {
        matches!(&status.station, StationState::Failed { ssid, reason: StationFailure::WrongPassword } if *ssid == network.ssid)
            || network.last == Some(LastAttempt::WrongPassword)
    }

    /// The board does not hear the network: a scan missed it, or the
    /// station's last try did.
    fn out_of_range(&self, status: &NetworkStatus, network: &SavedNetworkInfo) -> bool {
        self.heard.is_some()
            || network.last == Some(LastAttempt::NotFound)
            || matches!(&status.station, StationState::Failed { ssid, reason: StationFailure::NotFound } if *ssid == network.ssid)
    }
}

/// A link below author: nothing is asked, nothing offered.
pub const NEEDS_AUTHOR: &str = "Needs Author access — unlock with an author password.";

/// The status is on its way.
pub const READING: &str = "Reading…";

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(ssid: &str, last: Option<LastAttempt>) -> SavedNetworkInfo {
        SavedNetworkInfo {
            ssid: ssid.to_string(),
            has_password: true,
            hidden: false,
            last,
        }
    }

    fn heard(ssid: &str, rssi: i8) -> HeardNetwork {
        HeardNetwork {
            ssid: ssid.to_string(),
            rssi,
            secure: true,
        }
    }

    fn wifi(station: StationState, networks: Vec<SavedNetworkInfo>) -> UiDeviceWifi {
        UiDeviceWifi {
            status: Some(NetworkStatus {
                wifi: true,
                cloud_relay: true,
                networks,
                station,
                relay: lpc_wire::RelayState::Off,
            }),
            ..UiDeviceWifi::new(DeviceId(1), true)
        }
    }

    fn words(wifi: &UiDeviceWifi) -> Vec<(String, String)> {
        wifi.rows()
            .into_iter()
            .map(|row| (row.ssid, row.word))
            .collect()
    }

    #[test]
    fn the_connected_network_leads_then_the_rest_in_saved_order() {
        let mut wifi = wifi(
            StationState::Connected {
                ssid: "Starlink Truck".to_string(),
                ip: "192.168.1.17".to_string(),
                rssi: -41,
                host: "lp-8e30.local".to_string(),
            },
            vec![
                saved("Starlink Home", None),
                saved("Starlink Truck", Some(LastAttempt::Connected)),
                saved("Starlink Apt", Some(LastAttempt::WrongPassword)),
                saved("Back Office", None),
            ],
        );
        wifi.heard = Some(vec![
            heard("Starlink Truck", -41),
            heard("Back Office", -70),
        ]);
        assert_eq!(
            words(&wifi),
            [
                (
                    "Starlink Truck".to_string(),
                    "Connected · 192.168.1.17".to_string()
                ),
                ("Starlink Home".to_string(), "Not in range".to_string()),
                ("Starlink Apt".to_string(), "Wrong password".to_string()),
                ("Back Office".to_string(), "In range".to_string()),
            ]
        );
        assert!(wifi.rows()[0].in_use);
        assert_eq!(wifi.row_value(), "Starlink Truck");
        assert_eq!(wifi.row_tone(), WifiTone::Good);
        assert_eq!(wifi.networks_line(), None, "connected: no line");
        assert_eq!(wifi.rows()[0].slug, "starlink-truck");
    }

    #[test]
    fn todays_firmware_says_saved_everywhere() {
        let wifi = wifi(
            StationState::Unsupported,
            vec![saved("lp-walk-net", None), saved("lp-back-office", None)],
        );
        assert!(!wifi.can_connect());
        assert!(wifi.rows().iter().all(|row| row.word == "Saved"));
        assert_eq!(wifi.row_value(), "2 saved");
        assert_eq!(
            wifi.networks_line().as_deref(),
            Some(wifi_words::NOT_CONNECTED_UNSUPPORTED)
        );
        assert_eq!(
            wifi.network_line("lp-walk-net"),
            Some((page::SAVED_UNSUPPORTED.to_string(), WifiTone::Plain))
        );
        assert_eq!(
            wifi.password_line("lp-walk-net"),
            Some(wifi_words::PASSWORD_SAVED)
        );
    }

    #[test]
    fn nothing_saved_opens_on_connect_and_the_row_says_set_up() {
        let wifi = wifi(StationState::Unsupported, Vec::new());
        assert!(wifi.nothing_saved());
        assert_eq!(wifi.row_value(), "set up");
        let mut play = UiDeviceWifi::new(DeviceId(1), false);
        assert_eq!(play.row_value(), "needs author");
        assert_eq!(play.waiting_line(), Some(NEEDS_AUTHOR));
        play.can_edit = true;
        assert_eq!(play.waiting_line(), Some(READING));
    }

    #[test]
    fn the_test_shows_in_its_row_and_hides_the_line() {
        let mut wifi = wifi(StationState::Unsupported, vec![saved("lp-walk-net", None)]);
        wifi.testing = Some("lp-walk-net".to_string());
        let test = wifi.test().expect("a test row");
        assert_eq!(test.result().unwrap().headline, "Saved");
        assert_eq!(wifi.networks_line(), None);
        wifi.testing = Some("forgotten-net".to_string());
        assert_eq!(wifi.test(), None, "a network no longer saved has no test");
    }

    #[test]
    fn nearby_leaves_out_the_saved_and_puts_the_strongest_first() {
        let mut wifi = wifi(
            StationState::NotConnected,
            vec![saved("Starlink Home", None)],
        );
        wifi.heard = Some(vec![
            heard("NETGEAR42", -71),
            heard("Starlink Home", -48),
            HeardNetwork {
                secure: false,
                ..heard("xfinitywifi", -60)
            },
        ]);
        let names: Vec<String> = wifi.nearby().into_iter().map(|n| n.ssid).collect();
        assert_eq!(names, ["xfinitywifi", "NETGEAR42"]);
        assert!(wifi.heard_open("xfinitywifi"));
        assert!(!wifi.heard_open("NETGEAR42"));
        assert_eq!(
            wifi.network_line("Starlink Home"),
            Some(("In range · strong signal".to_string(), WifiTone::Plain))
        );
    }
}
