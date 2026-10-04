//! The device's network settings, read and changed on the board (wire
//! proto 36's `NetworkStatus` / `NetworkSet` / `NetworkForget`, all edit
//! tier).
//!
//! Each request is answered with the status as it now stands — the saved
//! network without its password, `lanOnly`, and what the station is doing —
//! so one conversation is one request. [`run_network_step`] runs it and
//! says how it went as a [`NetworkCommand`], posted back onto the actor's
//! queue like every other device conversation.

use lpa_client::{ClientError, ClientIo, LpClient};
use lpa_devices::DeviceId;
use lpc_wire::server::NetworkStatus;

use super::network_command::{NetworkCommand, NetworkStepKind};
use super::wifi_password_change::PasswordChange;

/// One network conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkStep {
    /// Read the status.
    Read,
    /// `NetworkSet`; `None` leaves a setting. `Debug` stays safe:
    /// [`PasswordChange`] never prints the password.
    Set {
        ssid: Option<String>,
        password: PasswordChange,
        enabled: Option<bool>,
        lan_only: Option<bool>,
    },
    /// `NetworkForget`.
    Forget,
}

impl NetworkStep {
    /// Whether the step changes the board (a write), or only reads.
    pub fn kind(&self) -> NetworkStepKind {
        match self {
            Self::Read => NetworkStepKind::Read,
            Self::Set { .. } | Self::Forget => NetworkStepKind::Write,
        }
    }
}

/// Run `step` on `client` and say how it went.
pub async fn run_network_step<Io: ClientIo>(
    client: &mut LpClient<Io>,
    device: DeviceId,
    step: NetworkStep,
) -> NetworkCommand {
    let kind = step.kind();
    let result = match step {
        NetworkStep::Read => client.network_status().await,
        NetworkStep::Set {
            ssid,
            password,
            enabled,
            lan_only,
        } => {
            client
                .network_set(ssid, password.to_wire(), enabled, lan_only)
                .await
        }
        NetworkStep::Forget => client.network_forget().await,
    };
    NetworkCommand::Answered {
        device,
        kind,
        result: result.map(|outcome| outcome.value).map_err(refusal),
    }
}

/// Why a request did not answer a status, in words for the popover. A
/// refusal for want of a tier is its own case: the popover says what it
/// needs instead of an error.
fn refusal(error: ClientError) -> NetworkRefusal {
    match error {
        ClientError::NotPermitted { needs } => NetworkRefusal::NotPermitted(needs),
        // The board's own sentence: it names the rule, never the password.
        ClientError::Server(error) => NetworkRefusal::Said(error),
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

/// What a network answer leaves the board's status at, for tests and the
/// controller: the status the board answered, or why not.
pub type NetworkAnswer = Result<NetworkStatus, NetworkRefusal>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::test_board::{FakeBoard, block_on};
    use lpc_wire::server::{StationState, WifiInfo};

    fn answer(command: NetworkCommand) -> NetworkAnswer {
        let NetworkCommand::Answered { result, .. } = command else {
            panic!("{command:?}");
        };
        result
    }

    fn set(ssid: Option<&str>, password: PasswordChange) -> NetworkStep {
        NetworkStep::Set {
            ssid: ssid.map(str::to_string),
            password,
            enabled: None,
            lan_only: None,
        }
    }

    #[test]
    fn set_read_and_forget_round_trip_over_usb() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let read = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Read,
        )))
        .unwrap();
        assert_eq!(read.wifi, None);
        assert_eq!(read.station, StationState::Unsupported);

        let saved = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            set(
                Some("lp-walk-net"),
                PasswordChange::Set("correct-horse-42".to_string()),
            ),
        )))
        .unwrap();
        assert_eq!(
            saved.wifi,
            Some(WifiInfo {
                ssid: "lp-walk-net".to_string(),
                has_password: true,
                enabled: true,
            })
        );
        assert_eq!(
            board.network().wifi.unwrap().password,
            "correct-horse-42",
            "the board holds it"
        );

        let forgot = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Forget,
        )))
        .unwrap();
        assert_eq!(forgot.wifi, None);
    }

    #[test]
    fn a_new_network_without_a_password_is_refused_in_the_boards_words() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        let refused = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            set(Some("lp-walk-net"), PasswordChange::Keep),
        )))
        .unwrap_err();
        let NetworkRefusal::Said(sentence) = refused else {
            panic!("{refused:?}");
        };
        assert!(sentence.contains("password"), "{sentence}");
        assert_eq!(board.network().wifi, None, "nothing written");
    }

    /// M6's station states pass through verbatim, and read as their
    /// sentence.
    #[test]
    fn the_station_is_reported_as_the_board_says() {
        let board = FakeBoard::fresh();
        let mut usb = board.usb();
        block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            set(
                Some("lp-walk-net"),
                PasswordChange::Set("correct-horse-42".to_string()),
            ),
        ));
        board.set_station(StationState::Joined {
            ip: "192.168.1.40".to_string(),
            rssi: -58,
        });
        let status = answer(block_on(run_network_step(
            &mut usb,
            DeviceId(1),
            NetworkStep::Read,
        )))
        .unwrap();
        assert_eq!(
            crate::app::network::wifi_status_sentence(&status),
            "Joined lp-walk-net · 192.168.1.40 · -58 dBm"
        );
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
