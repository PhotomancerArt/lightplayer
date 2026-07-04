//! Passive-refresh cadence policy, as data, in core.
//!
//! The UI's refresh timer enqueues a [`StudioCommand::RefreshTick`] at an
//! interval. That interval used to be a `LinkProviderKind` match in the web
//! crate (the retired `ProjectRefreshCadence` enum + `project_refresh_interval_ms`
//! functions); P4 moves it here so the view layer holds no transport-sniffing
//! policy. It is expressed as data on the connection state ([`LinkState`]):
//! `for_link_state` does the one legitimate provider match, in core.
//!
//! Per M7 Q3 the default is a single uniform cadence; the browser simulator keeps
//! a faster interval only because it self-ticks and the UI re-reads previews at
//! that rate (see the simulator-clock ADR), while a real device polls calmly.

use core::time::Duration;

use lpa_link::LinkProviderKind;

use crate::LinkState;

/// Fast interval for the self-ticking browser simulator: the UI re-reads preview
/// state at ~30 Hz so self-ticked previews stay visibly fresh. Retired web
/// constant `SIMULATOR_PROJECT_REFRESH_INTERVAL_MS`.
pub const SIMULATOR_REFRESH_INTERVAL: Duration = Duration::from_millis(33);

/// Calm interval for a real connected device (and the default when no device is
/// connected). Retired web constant `DEVICE_PROJECT_REFRESH_INTERVAL_MS`.
pub const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_millis(750);

/// The passive-refresh cadence for a connection: the interval the UI timer waits
/// between enqueuing refresh ticks.
///
/// This is data, not behaviour: [`Self::for_link_state`] derives it from the
/// current [`LinkState`] in core, and the UI timer just reads
/// [`Self::interval`]. There is no `LinkProviderKind` match left in the view
/// layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefreshCadence {
    interval: Duration,
}

impl RefreshCadence {
    /// The default (device) cadence, used before a simulator connects.
    pub const fn device() -> Self {
        Self {
            interval: DEVICE_REFRESH_INTERVAL,
        }
    }

    /// The simulator cadence.
    pub const fn simulator() -> Self {
        Self {
            interval: SIMULATOR_REFRESH_INTERVAL,
        }
    }

    /// Derive the cadence from the current connection state. The browser-worker
    /// simulator gets the fast interval; everything else gets the device
    /// interval.
    pub fn for_link_state(state: &LinkState) -> Self {
        match state {
            LinkState::Connected { device } | LinkState::Managing { device, .. }
                if device.provider_id == LinkProviderKind::BrowserWorker =>
            {
                Self::simulator()
            }
            _ => Self::device(),
        }
    }

    /// The interval the UI timer waits between refresh ticks.
    pub fn interval(self) -> Duration {
        self.interval
    }
}

impl Default for RefreshCadence {
    fn default() -> Self {
        Self::device()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectedDeviceSummary, ProgressState};

    fn connected(provider: LinkProviderKind) -> LinkState {
        LinkState::Connected {
            device: ConnectedDeviceSummary::new(provider, "endpoint", "session", "label"),
        }
    }

    #[test]
    fn browser_worker_link_uses_simulator_cadence() {
        let cadence = RefreshCadence::for_link_state(&connected(LinkProviderKind::BrowserWorker));

        assert_eq!(cadence.interval(), SIMULATOR_REFRESH_INTERVAL);
    }

    #[test]
    fn serial_link_uses_device_cadence() {
        let cadence =
            RefreshCadence::for_link_state(&connected(LinkProviderKind::BrowserSerialEsp32));

        assert_eq!(cadence.interval(), DEVICE_REFRESH_INTERVAL);
    }

    #[test]
    fn managing_browser_worker_keeps_simulator_cadence() {
        let state = LinkState::Managing {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::BrowserWorker,
                "endpoint",
                "session",
                "Simulator",
            ),
            progress: ProgressState::new("Resetting simulator"),
        };

        assert_eq!(
            RefreshCadence::for_link_state(&state).interval(),
            SIMULATOR_REFRESH_INTERVAL
        );
    }

    #[test]
    fn no_connection_defaults_to_device_cadence() {
        assert_eq!(
            RefreshCadence::default().interval(),
            DEVICE_REFRESH_INTERVAL
        );
    }
}
