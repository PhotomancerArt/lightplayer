//! Passive-refresh cadence policy, as data, in core.
//!
//! The UI's refresh timer enqueues a [`StudioCommand::RefreshTick`] at an
//! interval published by the actor. That interval used to be a
//! `LinkProviderKind` match in the web crate (the retired
//! `ProjectRefreshCadence` enum + `project_refresh_interval_ms` functions);
//! P4 moved it into core, and the runtime pool's P2 made it **per session**:
//! cadence derives from each [`RuntimeSession`](crate::RuntimeSession)'s
//! KIND ([`RefreshCadence::for_kind`]), the lens session drives the
//! project-refresh tick, non-lens device sessions get the slow
//! [`DEVICE_HEARTBEAT_INTERVAL`] status heartbeat, and the actor's
//! published delay is the minimum over sessions
//! (`StudioController::next_refresh_interval`).
//!
//! Per M7 Q3 the default is a single uniform cadence; the browser simulator
//! keeps a faster interval only because it self-ticks and the UI re-reads
//! previews at that rate (see the simulator-clock ADR), while a real device
//! polls calmly.

use core::time::Duration;

use crate::RuntimeKind;

/// Fast interval for the self-ticking browser simulator: the UI re-reads preview
/// state at ~30 Hz so self-ticked previews stay visibly fresh. Retired web
/// constant `SIMULATOR_PROJECT_REFRESH_INTERVAL_MS`.
pub const SIMULATOR_REFRESH_INTERVAL: Duration = Duration::from_millis(33);

/// Calm interval for a real connected device (and the default when no device is
/// connected). Retired web constant `DEVICE_PROJECT_REFRESH_INTERVAL_MS`.
pub const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_millis(750);

/// Slow status-heartbeat interval for DEVICE sessions the editor lens is not
/// on (runtime-pool P2): each heartbeat drains the session's buffered wire
/// and console log lines and surfaces device-state changes to the change
/// gate. No wire operation rides a heartbeat — the device session's monitor
/// fills the buffers in the background.
pub const DEVICE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

/// The default passive-refresh backoff base: start at 3 s (the retired flat
/// `PASSIVE_REFRESH_FAILURE_BACKOFF_MS`), double on consecutive failures, cap
/// at [`PASSIVE_REFRESH_BACKOFF_MAX`]. Each session carries its own
/// `BackoffPolicy` built from these (runtime-pool P2); only the lens
/// session's advances, since only the lens runs the fallible project pull.
pub const PASSIVE_REFRESH_BACKOFF_BASE: Duration = Duration::from_secs(3);
/// Cap for [`PASSIVE_REFRESH_BACKOFF_BASE`] exponential backoff.
pub const PASSIVE_REFRESH_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Tightened passive-tick interval while an accepted asset-body apply awaits
/// its compile verdict (the shader auto-apply plan's post-ack refresh): the
/// device compiles on its next engine frame (~200 ms), so a couple of quick
/// pulls surface the error/clean verdict without waiting a full
/// [`DEVICE_REFRESH_INTERVAL`]. Only ever *tightens* the cadence — the
/// simulator's 33 ms interval stays as-is.
pub const VERDICT_CHASE_INTERVAL: Duration = Duration::from_millis(250);

/// How many passive ticks run at [`VERDICT_CHASE_INTERVAL`] after an accepted
/// apply before the cadence relaxes back to the connection policy.
pub const VERDICT_CHASE_TICKS: u8 = 3;

/// The passive-refresh cadence for one runtime session: the interval the UI
/// timer waits between enqueuing refresh ticks while the editor lens is on
/// that session.
///
/// This is data, not behaviour: [`Self::for_kind`] derives it from the
/// session's [`RuntimeKind`] in core, and the UI timer just reads the delay
/// the actor publishes. There is no `LinkProviderKind` match left in the
/// view layer, and no shared flow-state singleton left in core (the retired
/// `for_flow_state` read the one connect flow — a single-session
/// assumption the runtime pool removed).
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

    /// Derive the cadence from a session's runtime kind. The browser-worker
    /// simulator gets the fast interval; hardware gets the device interval.
    pub fn for_kind(kind: RuntimeKind) -> Self {
        match kind {
            RuntimeKind::Sim => Self::simulator(),
            RuntimeKind::Device => Self::device(),
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

    #[test]
    fn sim_sessions_use_the_simulator_cadence() {
        let cadence = RefreshCadence::for_kind(RuntimeKind::Sim);

        assert_eq!(cadence.interval(), SIMULATOR_REFRESH_INTERVAL);
    }

    #[test]
    fn device_sessions_use_the_device_cadence() {
        let cadence = RefreshCadence::for_kind(RuntimeKind::Device);

        assert_eq!(cadence.interval(), DEVICE_REFRESH_INTERVAL);
    }

    #[test]
    fn no_connection_defaults_to_device_cadence() {
        assert_eq!(
            RefreshCadence::default().interval(),
            DEVICE_REFRESH_INTERVAL
        );
    }

    #[test]
    fn heartbeat_is_slower_than_every_lens_cadence() {
        // The heartbeat is the slow lane: a device session the lens IS on
        // already ticks at the (faster) lens cadence, so heartbeats only
        // ever add drains, never tighten the timer.
        assert!(DEVICE_HEARTBEAT_INTERVAL > DEVICE_REFRESH_INTERVAL);
        assert!(DEVICE_HEARTBEAT_INTERVAL > SIMULATOR_REFRESH_INTERVAL);
    }
}
