//! The device's network settings, read and changed on the board (wire
//! proto 36's `NetworkStatus` / `NetworkScan` / `NetworkAdd` /
//! `NetworkForget` / `NetworkSet`, all edit tier).
//!
//! Each change is answered with the status as it now stands — the switches,
//! every saved network without its password, and what the station is
//! doing — so one conversation is one request. [`run_network_step`] runs it
//! and says how it went as a [`NetworkCommand`], posted back onto the
//! actor's queue like every other device conversation.

use lpa_client::{ClientError, ClientIo, LpClient};
use lpa_devices::DeviceId;
use lpc_access::NetworkFileError;
use lpc_wire::server::{NetworkScan, NetworkStatus};

use super::network_command::{NetworkCommand, NetworkStepKind};
use super::wifi_password_change::PasswordChange;

/// One network conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkStep {
    /// Read the status.
    Read,
    /// Ask what the radio hears.
    Scan,
    /// `NetworkAdd`. `Debug` stays safe: [`PasswordChange`] never prints the
    /// password.
    Add {
        ssid: String,
        password: PasswordChange,
        hidden: Option<bool>,
    },
    /// `NetworkForget`.
    Forget { ssid: String },
    /// `NetworkSet`; `None` leaves a switch.
    Switches {
        wifi: Option<bool>,
        cloud_relay: Option<bool>,
    },
}

/// Run `step` on `client` and say how it went.
pub async fn run_network_step<Io: ClientIo>(
    client: &mut LpClient<Io>,
    device: DeviceId,
    step: NetworkStep,
) -> NetworkCommand {
    let (kind, result) = match step {
        NetworkStep::Scan => {
            let result = client.network_scan().await;
            return NetworkCommand::Scanned {
                device,
                result: result.map(|outcome| outcome.value).map_err(refusal),
            };
        }
        NetworkStep::Read => (NetworkStepKind::Read, client.network_status().await),
        NetworkStep::Add {
            ssid,
            password,
            hidden,
        } => (
            NetworkStepKind::Write,
            client.network_add(ssid, password.to_wire(), hidden).await,
        ),
        NetworkStep::Forget { ssid } => (NetworkStepKind::Write, client.network_forget(ssid).await),
        NetworkStep::Switches { wifi, cloud_relay } => (
            NetworkStepKind::Write,
            client.network_set(wifi, cloud_relay).await,
        ),
    };
    NetworkCommand::Answered {
        device,
        kind,
        result: result.map(|outcome| outcome.value).map_err(refusal),
    }
}

/// Why a request did not answer, in words for the popover. A refusal for
/// want of a tier is its own case: the popover says what it needs instead
/// of an error.
fn refusal(error: ClientError) -> NetworkRefusal {
    match error {
        ClientError::NotPermitted { needs } => NetworkRefusal::NotPermitted(needs),
        // The board sends a bare code, never a sentence (cheap on the
        // device); `reword_refusal` turns it back into words here, off
        // the device, so no user-visible text ever shows a raw code.
        // Studio's offer binder already turned the common cases into
        // words before sending (`wifi_offers::bind_add`), so this path
        // is the rare one: a caller with no early check of its own, or a
        // race between two clients.
        ClientError::Server(error) => {
            NetworkRefusal::Said(NetworkFileError::reword_refusal(&error))
        }
        error => NetworkRefusal::Said(format!("the device did not answer: {error}")),
    }
}

/// How a network request was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkRefusal {
    /// The link does not hold the tier the request needs.
    NotPermitted(lpc_access::Tier),
    /// Anything else, in words (the board's sentence, a lost link).
    Said(String),
}

/// What a network answer leaves the board's status at: the status the
/// board answered, or why not.
pub type NetworkAnswer = Result<NetworkStatus, NetworkRefusal>;

/// What a scan answered: what the radio heard (or `unsupported`), or why
/// not.
pub type ScanAnswer = Result<NetworkScan, NetworkRefusal>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::test_board::{FakeBoard, block_on};
    use lpc_wire::server::{HeardNetwork, SavedNetworkInfo, StationState};

    fn answer(command: NetworkCommand) -> NetworkAnswer {
        let NetworkCommand::Answered { result, .. } = command else {
            panic!("{command:?}");
        };
        result
    }

    fn add(ssid: &str, password: PasswordChange) -> NetworkStep {
        NetworkStep::Add {
            ssid: ssid.to_string(),
            password,
            hidden: None,
        }
    }

    #[test]
    fn add_read_and_forget_round_trip_over_usb() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let read = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Read,
        )))
        .unwrap();
        assert!(read.networks.is_empty());
        assert_eq!(read.station, StationState::Unsupported);

        block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            add(
                "lp-walk-net",
                PasswordChange::Set("correct-horse-42".to_string()),
            ),
        ));
        let saved = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            add("lp-cafe", PasswordChange::Open),
        )))
        .unwrap();
        assert_eq!(
            saved.networks,
            [
                SavedNetworkInfo {
                    ssid: "lp-walk-net".to_string(),
                    has_password: true,
                    hidden: false,
                    last: None,
                },
                SavedNetworkInfo {
                    ssid: "lp-cafe".to_string(),
                    has_password: false,
                    hidden: false,
                    last: None,
                },
            ]
        );
        assert_eq!(
            board.network().networks[0].password,
            "correct-horse-42",
            "the board holds it"
        );

        let forgot = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Forget {
                ssid: "lp-walk-net".to_string(),
            },
        )))
        .unwrap();
        assert_eq!(forgot.networks.len(), 1);
        let switched = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Switches {
                wifi: Some(false),
                cloud_relay: None,
            },
        )))
        .unwrap();
        assert!(!switched.wifi);
        assert!(switched.cloud_relay);
    }

    #[test]
    fn a_short_password_is_refused_in_the_boards_words() {
        // A conversation run directly (no offer binder in front, as
        // `lp-cli` runs it): the board sends a bare code, and `refusal`
        // (`NetworkFileError::reword_refusal`) turns it back into words
        // here — a raw code never reaches this far.
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let refused = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            add("lp-walk-net", PasswordChange::Set("short".to_string())),
        )))
        .unwrap_err();
        let NetworkRefusal::Said(sentence) = refused else {
            panic!("{refused:?}");
        };
        assert!(sentence.contains("too short"), "{sentence}");
        assert!(!sentence.contains("passwordTooShort"), "{sentence}");
        assert!(board.network().networks.is_empty(), "nothing written");
    }

    /// A scan answers what the fake radio hears, and `unsupported` by
    /// default.
    #[test]
    fn a_scan_passes_through() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let scan =
            |usb: &mut _| match block_on(run_network_step(usb, DeviceId(1), NetworkStep::Scan)) {
                NetworkCommand::Scanned { result, .. } => result.unwrap(),
                other => panic!("{other:?}"),
            };
        assert_eq!(scan(&mut usb), NetworkScan::Unsupported);
        let heard = NetworkScan::Heard(vec![HeardNetwork {
            ssid: "lp-walk-net".to_string(),
            rssi: -48,
            secure: true,
        }]);
        board.set_scan(heard.clone());
        assert_eq!(scan(&mut usb), heard);
    }

    /// M6's station, faked: the board reports each state verbatim, and the
    /// row and its test read them in their words (2B: the test runs in the
    /// new network's row).
    #[test]
    fn a_fake_station_walks_the_row_and_its_test_through_every_word() {
        use crate::app::network::{UiDeviceWifi, WifiTone};
        use lpc_wire::server::{LastAttempt, StationFailure};

        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        for ssid in ["Starlink Home", "Starlink Apt"] {
            block_on(run_network_step(
                &mut usb,
                DeviceId(1),
                add(ssid, PasswordChange::Set("correct-horse-42".to_string())),
            ));
        }
        let mut read = |station: StationState| {
            board.set_station(station);
            let status = answer(block_on(run_network_step(
                &mut usb,
                DeviceId(1),
                NetworkStep::Read,
            )))
            .unwrap();
            UiDeviceWifi {
                status: Some(status),
                testing: Some("Starlink Apt".to_string()),
                ..UiDeviceWifi::new(DeviceId(1), true)
            }
        };
        let words = |wifi: &UiDeviceWifi| -> Vec<String> {
            wifi.rows().into_iter().map(|row| row.word).collect()
        };

        let wifi = read(StationState::Connecting {
            ssid: "Starlink Apt".to_string(),
            step: lpc_wire::ConnectStep::Looking,
        });
        assert_eq!(words(&wifi), ["Saved", "Connecting…"]);
        let test = wifi.test().unwrap();
        assert_eq!(test.steps()[0].label, "Looking for Starlink Apt");
        assert_eq!(test.result(), None, "still running");

        let wifi = read(StationState::Connected {
            ssid: "Starlink Apt".to_string(),
            ip: "10.0.0.23".to_string(),
            rssi: -57,
            host: "lp-8e30.local".to_string(),
        });
        assert_eq!(words(&wifi), ["Connected · 10.0.0.23", "Saved"]);
        assert_eq!(wifi.rows()[0].tone, WifiTone::Good);
        assert_eq!(wifi.row_value(), "Starlink Apt");
        assert_eq!(
            wifi.test().unwrap().result().unwrap().body,
            "good signal · 10.0.0.23"
        );

        let wifi = read(StationState::Failed {
            ssid: "Starlink Apt".to_string(),
            reason: StationFailure::WrongPassword,
        });
        assert_eq!(words(&wifi), ["Saved", "Wrong password"]);
        assert_eq!(wifi.row_value(), "wrong password");
        assert_eq!(
            wifi.test().unwrap().result().unwrap().headline,
            "Wrong password"
        );

        board.set_last("Starlink Home", LastAttempt::NotFound);
        let wifi = read(StationState::NotConnected);
        assert_eq!(words(&wifi)[0], "Not in range");
        assert_eq!(wifi.networks_line(), None, "the test row speaks instead");
        let wifi = UiDeviceWifi {
            testing: None,
            ..wifi
        };
        assert_eq!(wifi.networks_line().as_deref(), Some("Not connected."));

        let wifi = read(StationState::Off);
        assert_eq!(words(&wifi), ["Saved", "Saved"]);
        assert_eq!(wifi.test().unwrap().result().unwrap().headline, "Saved");
    }

    #[test]
    fn a_locked_link_is_refused_for_want_of_author() {
        let board = FakeBoard::locked(&[("camp", lpc_access::Tier::Play, "x")]);
        let mut ble = board.client();
        let refused = answer(block_on(run_network_step(
            &mut ble,
            DeviceId(1),
            NetworkStep::Read,
        )))
        .unwrap_err();
        assert_eq!(
            refused,
            NetworkRefusal::NotPermitted(lpc_access::Tier::Edit)
        );
    }
}
