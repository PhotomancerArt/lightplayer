//! The Radio node rule: ESP-NOW is off while the board uses Wi-Fi.

use alloc::string::String;
use lpc_hardware::{HardwareEndpointError, HwEndpointId, HwEndpointStatus};

/// What a Radio node says while the board is set to use Wi-Fi (plan Q2,
/// MD8, roadmap D8): why the Radio is off and how to get it back. The rest
/// of the project keeps running.
pub const RADIO_OFF_FOR_WIFI: &str =
    "Radio is off while this board uses Wi-Fi. Turn Wi-Fi off for this board to use Radio.";

/// The ESP-NOW endpoint's status under the rule: unavailable, in
/// [`RADIO_OFF_FOR_WIFI`]'s words, while `uses_wifi` (the Wi-Fi switch on
/// and a network saved, [`crate::net::StationSettings::uses_wifi`]); else
/// what the registry says.
///
/// Why "set to use Wi-Fi" and not "joined": a board searching for its
/// networks hops channels on every scan, and a rule that followed the
/// router would flap the Radio every time it dropped. A board with nothing
/// saved never scans, so ESP-NOW keeps its channel (the fyeah sign).
#[must_use]
pub fn radio_endpoint_status(uses_wifi: bool, registry: HwEndpointStatus) -> HwEndpointStatus {
    if uses_wifi {
        HwEndpointStatus::Unavailable {
            reason: String::from(RADIO_OFF_FOR_WIFI),
        }
    } else {
        registry
    }
}

/// The error an open Radio device answers with while the rule holds: the
/// node shows it and keeps its device, so the Radio comes back the moment
/// Wi-Fi is turned off (the ESP-NOW interface opens once per boot).
#[must_use]
pub fn radio_off_error(endpoint_id: HwEndpointId) -> HardwareEndpointError {
    HardwareEndpointError::EndpointUnavailable {
        endpoint_id,
        reason: String::from(RADIO_OFF_FOR_WIFI),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn set_to_use_wifi_turns_the_radio_off_in_words() {
        let status = radio_endpoint_status(true, HwEndpointStatus::Available);
        assert_eq!(status.unavailable_reason(), Some(RADIO_OFF_FOR_WIFI));
        assert_eq!(
            radio_endpoint_status(false, HwEndpointStatus::Available),
            HwEndpointStatus::Available
        );
    }

    #[test]
    fn the_open_device_says_why_and_how_to_get_it_back() {
        let error = radio_off_error(HwEndpointId::new("radio:local:0")).to_string();
        assert!(
            error.contains("Radio is off while this board uses Wi-Fi"),
            "{error}"
        );
        assert!(error.contains("Turn Wi-Fi off"), "{error}");
    }
}
