//! The station's radio controls over esp-radio's `WifiController`.

use alloc::string::String;
use alloc::vec::Vec;

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethod, Config, DisconnectReason, PowerSaveMode, SecondaryChannel,
    WifiController, WifiError,
};
use fw_esp32_common::net::{ConnectOutcome, StationControl};
use lpc_wire::HeardNetwork;

/// How long a scan may take before it is given up (a real one takes about
/// two seconds; an emulated board's radio never finishes one).
const SCAN_LIMIT: Duration = Duration::from_secs(6);
/// How long one connect may take to be decided.
const CONNECT_LIMIT: Duration = Duration::from_secs(14);
/// How long a disconnect may take.
const DISCONNECT_LIMIT: Duration = Duration::from_secs(3);

/// [`StationControl`] over esp-radio (plan P03). Owns the controller for
/// the boot: dropping it stops Wi-Fi and ESP-NOW.
pub struct EspStation {
    controller: WifiController<'static>,
}

impl EspStation {
    pub fn new(controller: WifiController<'static>) -> Self {
        Self { controller }
    }

    /// Put the radio back on ESP-NOW's channel, after a scan or a leave
    /// while the board is not set to use Wi-Fi (an open Radio node keeps
    /// hearing its peers). Refused while joined, which is fine: a joined
    /// board's Radio is off.
    pub fn restore_espnow_channel(&mut self) {
        let channel = crate::hardware::espnow_radio_driver::DEFAULT_ESPNOW_CHANNEL;
        if self
            .controller
            .set_channel(channel, SecondaryChannel::None)
            .is_err()
        {
            log::warn!("[wifi] could not put the radio back on channel {channel}");
        }
    }
}

impl StationControl for EspStation {
    async fn scan(&mut self) -> Option<Vec<HeardNetwork>> {
        let config = ScanConfig::default();
        match select(self.controller.scan_async(&config), Timer::after(SCAN_LIMIT)).await {
            Either::First(Ok(found)) => Some(
                found
                    .into_iter()
                    .filter(|ap| !ap.ssid.as_str().is_empty())
                    .map(|ap| HeardNetwork {
                        ssid: String::from(ap.ssid.as_str()),
                        rssi: ap.signal_strength,
                        secure: !matches!(ap.auth_method, None | Some(AuthenticationMethod::None)),
                    })
                    .collect(),
            ),
            Either::First(Err(error)) => {
                log::warn!("[wifi] scan failed: {}", error_words(&error));
                None
            }
            Either::Second(()) => {
                log::warn!("[wifi] scan gave no answer in {} s", SCAN_LIMIT.as_secs());
                None
            }
        }
    }

    async fn connect(&mut self, ssid: &str, password: &str) -> ConnectOutcome {
        let auth = if password.is_empty() {
            AuthenticationMethod::None
        } else {
            AuthenticationMethod::Wpa2Personal
        };
        let config = Config::Station(
            StationConfig::default()
                .with_ssid(ssid)
                .with_auth_method(auth)
                .with_password(String::from(password)),
        );
        if let Err(error) = self.controller.set_config(&config) {
            log::warn!("[wifi] station config refused: {}", error_words(&error));
            return ConnectOutcome::Ended;
        }
        match select(self.controller.connect_async(), Timer::after(CONNECT_LIMIT)).await {
            Either::First(Ok(_)) => {
                // Power save off while joined (plan MD9): `minimum` stalled
                // a request 3 s on the bench, `none` coexists with BLE.
                if self
                    .controller
                    .set_power_saving(PowerSaveMode::None)
                    .is_err()
                {
                    log::warn!("[wifi] power save could not be turned off");
                }
                ConnectOutcome::Associated
            }
            Either::First(Err(WifiError::Disconnected(info))) => {
                let outcome = outcome_of(info.reason);
                log::info!(
                    "[wifi] attempt ended: {} ({})",
                    reason_words(info.reason),
                    outcome_words(outcome)
                );
                outcome
            }
            Either::First(Err(error)) => {
                log::warn!("[wifi] connect failed: {}", error_words(&error));
                ConnectOutcome::Ended
            }
            Either::Second(()) => {
                log::warn!(
                    "[wifi] connect gave no answer in {} s",
                    CONNECT_LIMIT.as_secs()
                );
                ConnectOutcome::NotHeard
            }
        }
    }

    async fn disconnect(&mut self) {
        if !self.controller.is_connected() {
            return;
        }
        let _ = select(self.controller.disconnect_async(), Timer::after(DISCONNECT_LIMIT)).await;
    }

    async fn wait_link_lost(&mut self) {
        match self.controller.wait_for_disconnect_async().await {
            Ok(info) => log::info!("[wifi] link lost: {}", reason_words(info.reason)),
            Err(_) => {}
        }
    }

    fn rssi(&self) -> Option<i8> {
        self.controller
            .rssi()
            .ok()
            .and_then(|rssi| i8::try_from(rssi).ok())
    }
}

/// What the station did with a disconnect `reason`: the ones a wrong
/// password produces (a WPA2 handshake that never completes, or a refused
/// authentication), the ones an absent network produces, and the rest.
fn outcome_of(reason: DisconnectReason) -> ConnectOutcome {
    match reason {
        DisconnectReason::FourWayHandshakeTimeout
        | DisconnectReason::HandshakeTimeout
        | DisconnectReason::AuthenticationFailed
        | DisconnectReason::MicFailure
        | DisconnectReason::_802_1xAuthenticationFailed => ConnectOutcome::AuthFailed,
        DisconnectReason::NoAccessPointFound
        | DisconnectReason::NoAccessPointFoundWithCompatibleSecurity
        | DisconnectReason::NoAccessPointFoundInAuthmodeThreshold
        | DisconnectReason::NoAccessPointFoundInRssiThreshold => ConnectOutcome::NotHeard,
        _ => ConnectOutcome::Ended,
    }
}

fn outcome_words(outcome: ConnectOutcome) -> &'static str {
    match outcome {
        ConnectOutcome::Associated => "associated",
        ConnectOutcome::AuthFailed => "the password was refused",
        ConnectOutcome::NotHeard => "not heard",
        ConnectOutcome::Ended => "ended",
    }
}

/// A disconnect reason in words for the log (Display: `{:?}` prints nothing
/// in this firmware's logs).
fn reason_words(reason: DisconnectReason) -> &'static str {
    match reason {
        DisconnectReason::FourWayHandshakeTimeout => "4-way handshake timeout",
        DisconnectReason::HandshakeTimeout => "handshake timeout",
        DisconnectReason::AuthenticationFailed => "authentication failed",
        DisconnectReason::MicFailure => "MIC failure",
        DisconnectReason::_802_1xAuthenticationFailed => "802.1X authentication failed",
        DisconnectReason::NoAccessPointFound => "no access point found",
        DisconnectReason::NoAccessPointFoundWithCompatibleSecurity => {
            "no access point with compatible security"
        }
        DisconnectReason::NoAccessPointFoundInAuthmodeThreshold => {
            "no access point in the auth threshold"
        }
        DisconnectReason::NoAccessPointFoundInRssiThreshold => {
            "no access point in the signal threshold"
        }
        DisconnectReason::BeaconTimeout => "beacon timeout",
        DisconnectReason::AssociationFailed => "association failed",
        DisconnectReason::ConnectionFailed => "connection failed",
        DisconnectReason::AuthenticationExpired => "authentication expired",
        DisconnectReason::AssociationLeave => "association leave",
        DisconnectReason::StationLeaving => "station leaving",
        _ => "other reason",
    }
}

fn error_words(error: &WifiError) -> &'static str {
    match error {
        WifiError::NotConnected => "not connected",
        WifiError::Disconnected(_) => "disconnected",
        WifiError::InvalidArguments => "invalid arguments",
        _ => "radio error",
    }
}
