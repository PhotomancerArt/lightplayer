//! Passive-refresh cadence policy, as data, in core.
//!
//! The UI's refresh timer enqueues a [`StudioCommand::RefreshTick`] at an
//! interval published by the actor. That interval used to be a
//! `LinkProviderKind` match in the web crate (the retired
//! `ProjectRefreshCadence` enum + `project_refresh_interval_ms` functions);
//! P4 moved it into core, and the runtime pool's P2 made it **per session**:
//! the lens session drives the project-refresh tick, non-lens sessions get
//! the slow [`DEVICE_HEARTBEAT_INTERVAL`] status heartbeat, and the actor's
//! published delay is the minimum over sessions
//! (`StudioController::next_refresh_interval`).
//!
//! **Completion-based pacing (probe-performance plan):** a cadence value is
//! the minimum GAP between one passive pull *completing* and the next one
//! starting — not a fixed period. The lens session stamps each pull's
//! completion time; the published delay counts down from that stamp, and an
//! early tick (the UI timer racing a slow pull) bounces off the due gate in
//! `refresh_loaded_project_tick_gated` as `ProjectRefreshOutcome::NotDue`
//! without touching the wire. A pull that takes longer than the gap
//! therefore pushes the next pull out instead of running back-to-back with
//! zero idle.
//!
//! Per M7 Q3 the default is a single uniform cadence; the browser sim
//! keeps a faster gap only because it self-ticks and the UI re-reads
//! previews at that rate (see the simulator-clock ADR), while a real device
//! polls calmly.

use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;

/// Fast completion-gap for the self-ticking browser sim: the UI re-reads
/// preview state at up to ~30 Hz so self-ticked previews stay visibly fresh.
/// Retired web constant `SIMULATOR_PROJECT_REFRESH_INTERVAL_MS`.
pub const SIMULATOR_REFRESH_INTERVAL: Duration = Duration::from_millis(33);

/// Completion-gap for a real connected device (and the default when no
/// device is connected). Under completion-based pacing this is idle time
/// BETWEEN pulls, not a period — a slow serial pull can no longer stack
/// behind the timer — so it is far tighter than the retired 750 ms fixed
/// interval. **75 ms since 2026-09-24**, chosen by feel on a real XIAO C6
/// at the JSON Pack desk sitting (lp2025/2026-09-23-1701-lp-json-pack, G1),
/// between 150, 75 and 33: a packed steady lens reply is ~0.7 KB against
/// ~2.4 KB of JSON, so the link time per read fell from ~27 ms to ~8 ms at
/// ~90 KB/s and the old 150 ms gap had become most of each read's cycle.
/// Retired web constant `DEVICE_PROJECT_REFRESH_INTERVAL_MS`.
pub const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_millis(75);

/// The most a [`set_device_lens_pause_override`] may ask for. The override
/// is a probe for the gap between reads, not a way to switch the lens off.
pub const DEVICE_LENS_PAUSE_OVERRIDE_MAX: Duration = Duration::from_millis(1_000);

/// `u32::MAX` = no override. See [`set_device_lens_pause_override`].
static DEVICE_LENS_PAUSE_OVERRIDE_MS: AtomicU32 = AtomicU32::new(u32::MAX);

/// A dev-only override for [`DEVICE_REFRESH_INTERVAL`] — the lens's pause
/// between a device read completing and the next one starting — for the
/// JSON Pack cadence probe (plan `lp-json-pack`, D2): Studio's
/// `?lens-pause-ms=N` sets it at page load so the same build can be felt at
/// 150, 75 and 33 ms. Clamped to 0–[`DEVICE_LENS_PAUSE_OVERRIDE_MAX`];
/// `None` clears it. Returns the pause now in force.
///
/// Not a setting: no UI, no persistence, and it moves nothing but the lens
/// cadence of a device session ([`RefreshCadence::device`]) — not the card's
/// frame feed, not the sim, not the heartbeat.
pub fn set_device_lens_pause_override(ms: Option<u64>) -> Duration {
    let stored = match ms {
        Some(ms) => clamp_lens_pause(ms).as_millis() as u32,
        None => u32::MAX,
    };
    DEVICE_LENS_PAUSE_OVERRIDE_MS.store(stored, Ordering::Relaxed);
    device_refresh_interval()
}

/// A requested lens pause, clamped to 0–[`DEVICE_LENS_PAUSE_OVERRIDE_MAX`].
pub fn clamp_lens_pause(ms: u64) -> Duration {
    Duration::from_millis(ms).min(DEVICE_LENS_PAUSE_OVERRIDE_MAX)
}

/// [`DEVICE_REFRESH_INTERVAL`], or the dev override when one is set.
pub fn device_refresh_interval() -> Duration {
    match DEVICE_LENS_PAUSE_OVERRIDE_MS.load(Ordering::Relaxed) {
        u32::MAX => DEVICE_REFRESH_INTERVAL,
        ms => Duration::from_millis(u64::from(ms)),
    }
}

/// How many passive runs in a row an arriving-command stream may cancel
/// before the next one is promoted to foreground standing and allowed to
/// finish.
///
/// **Preemption is priority, not starvation.** The pull loop's cancel signal
/// exists so a single user gesture never waits behind a background read —
/// a latency guarantee for one action. A LIVE CONTROL is not one action: a
/// fader/knob/tape drag is a continuous stream of foreground panel writes,
/// and under the plain rule every one of them cancels the in-flight pull at
/// its first frame boundary. The pull then never completes, never stamps
/// the pacing gate, and the preview freezes for the whole drag — measured
/// as zero canvas paints in the browser and 40 reads sent / 0 completed at
/// the actor (`a_drag_of_foreground_actions_does_not_starve_the_passive_pull`).
///
/// One is the smallest value that bounds the starvation: a gesture stream
/// cancels a pull, the next pull is protected and completes, so a drag
/// refreshes the preview at half the tick rate instead of never — on the
/// sim that is ~15 pulls/s, which is the rate it already achieves at
/// rest, so dragging no longer changes how live the preview looks. The cost
/// is bounded and symmetric: a gesture arriving during a promoted run waits
/// for that ONE pull to finish (~35 ms on the sim, one frame read on a
/// device), and only every other run. Recovery-class work — device reset,
/// disconnect — still preempts a promoted run, because it owns the
/// connection.
pub const PASSIVE_PREEMPTIONS_BEFORE_PROMOTION: u8 = 1;

/// Slack applied when deciding whether a passive pull is due: the UI timer
/// truncates the published delay to whole milliseconds, so a tick can fire
/// a hair "early". Anything within this window counts as due instead of
/// bouncing off the gate and re-arming a sub-millisecond timer.
pub const REFRESH_DUE_SLACK: Duration = Duration::from_millis(2);

/// Slow status-heartbeat interval for DEVICE sessions the editor lens is not
/// on (runtime-pool P2): each heartbeat drains the session's buffered wire
/// and console log lines and surfaces device-state changes to the change
/// gate. No wire operation rides a heartbeat — the device session's monitor
/// fills the buffers in the background.
pub const DEVICE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

/// Completion-gap for the device card's live frame feed (honest-device
/// preview): the minimum idle time between one published-frame read
/// COMPLETING and the next one starting, while a card's ▶ Play tab is
/// selected on a Ready device.
///
/// The lens's pre-2026-09-24 figure, kept at 150 ms when the lens
/// ([`DEVICE_REFRESH_INTERVAL`]) went to 75 — the card was not part of the
/// G1 feel test. The reasoning is the lens's —
/// under completion-based pacing the number is idle time, not a period, so
/// the frame size sets the real rate. One frame is `lamps × 3 × 2` bytes
/// before base64 (×4/3 after) on a link that carries every other protocol
/// message too: at the ~90 KB/s a USB-serial device sustains, a 1500-lamp
/// dome frame (~9 KB raw, ~12 KB encoded) takes ~130 ms to arrive and
/// settles at ~4–5 fps; a 300-lamp strip (~1.8 KB) lands in ~25 ms and hits
/// the 150 ms gap's ~5–7 fps ceiling. Big frames therefore self-throttle by
/// taking longer to complete, and shrinking this constant would not make
/// them faster — it would only remove the device's breathing room between
/// reads. Tune at the hardware feel-walk (G1), not here.
pub const DEVICE_CARD_FEED_INTERVAL: Duration = Duration::from_millis(150);

/// How old the newest device frame may get before the card's ▶ tab calls
/// it stale (amber "last frame · N s ago" instead of calm green).
///
/// Ratified at the UX spike gate (2026-08-05): five seconds is long enough
/// that a slow dome frame, a busy device, or a preempted feed pull never
/// flickers the treatment, and short enough that a board which actually
/// stopped publishing says so before anyone trusts a frozen picture.
/// Consumed by the ▶ tab renderer (P3) against
/// the card's frame-age line.
pub const FRAME_STALE_AFTER_SECS: f64 = 5.0;

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
/// sim's 33 ms interval stays as-is.
pub const VERDICT_CHASE_INTERVAL: Duration = Duration::from_millis(250);

/// How many passive ticks run at [`VERDICT_CHASE_INTERVAL`] after an accepted
/// apply before the cadence relaxes back to the connection policy.
pub const VERDICT_CHASE_TICKS: u8 = 3;

/// The passive-refresh cadence for one runtime session: the interval the UI
/// timer waits between enqueuing refresh ticks while the editor lens is on
/// that session.
///
/// This is data, not behaviour: core picks the cadence and the UI timer
/// just reads the delay the actor publishes. There is no
/// `LinkProviderKind` match left in the view layer, and no shared
/// flow-state singleton left in core (the retired `for_flow_state` read
/// the one connect flow — a single-session assumption the runtime pool
/// removed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefreshCadence {
    interval: Duration,
}

impl RefreshCadence {
    /// The default (device) cadence, used before a sim connects:
    /// [`DEVICE_REFRESH_INTERVAL`], unless the dev-only lens-pause override is
    /// set ([`set_device_lens_pause_override`]).
    pub fn device() -> Self {
        Self {
            interval: device_refresh_interval(),
        }
    }

    /// The sim cadence.
    pub const fn simulator() -> Self {
        Self {
            interval: SIMULATOR_REFRESH_INTERVAL,
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
    fn the_simulator_cadence_is_the_fast_one() {
        assert_eq!(
            RefreshCadence::simulator().interval(),
            SIMULATOR_REFRESH_INTERVAL
        );
        assert_eq!(RefreshCadence::device().interval(), DEVICE_REFRESH_INTERVAL);
    }

    #[test]
    fn no_connection_defaults_to_device_cadence() {
        assert_eq!(
            RefreshCadence::default().interval(),
            DEVICE_REFRESH_INTERVAL
        );
    }

    /// The override itself is process-wide, so it is never SET in a test:
    /// tests run in parallel and every device-cadence assertion would race
    /// it. The clamp is what there is to get wrong.
    #[test]
    fn the_lens_pause_override_is_clamped_to_a_second() {
        assert_eq!(clamp_lens_pause(75), Duration::from_millis(75));
        assert_eq!(clamp_lens_pause(0), Duration::ZERO);
        assert_eq!(clamp_lens_pause(60_000), DEVICE_LENS_PAUSE_OVERRIDE_MAX);
        assert_eq!(device_refresh_interval(), DEVICE_REFRESH_INTERVAL);
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
