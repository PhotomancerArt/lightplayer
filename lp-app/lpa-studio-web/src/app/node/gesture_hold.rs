//! The value under the hand, held until the app's view of it catches up.
//!
//! A panel control's displayed value comes back from core: the gesture
//! dispatches a write, the actor runs it, and the next view snapshot carries
//! the write's echo. Those snapshots lag the hand. A drag queues writes
//! faster than the actor runs them, so mid-drag snapshots carry the echo of an
//! EARLIER write — and a control that renders straight from them pulls its
//! thumb back to that older value. Worse, the last snapshot before the
//! release's own write lands is one of those stale ones, so a released
//! fader sat on an older value and then jumped to the one the hand let go
//! at ("it jumps back to its old value before taking"). The slower the link
//! (an emulated C6, a board over BLE), the longer the window.
//!
//! [`GestureHold`] is the widget-local fix, the same posture the knob's
//! drag preview already took (the fader, the knob, the slot slider, the XY
//! pad and the palette cycle's Step slider all use it; the clock tape's scrub
//! does the same thing inside its paint driver, with [`caught_up`]): while a gesture is live, and after it ends
//! until the view reports the value the hand wrote, the control shows the
//! held value instead of the snapshot. Display only — every write still goes
//! through the control's normal dispatch, and probe truth wins again the
//! moment it agrees (or, if the engine changed the value — clamping, a
//! refused write — once [`GESTURE_HOLD_MS`] passes with no new write).

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

/// How long a held value outlives the gesture's last write when the view
/// never reports it back (a clamped or refused write). Long enough to cover
/// a slow link's write + read round trip; short enough that a value the
/// engine changed under the hand still shows through promptly.
pub(crate) const GESTURE_HOLD_MS: u32 = 3_000;

/// How far a reported value may sit from the held one and still count as
/// the hand's write come back. Live bus readings are quantized to 0.01
/// before they reach a view (`format_live_scalar`), so an echo of 0.7134
/// arrives as 0.71; half a quantum plus float slack covers that.
const CAUGHT_UP_TOLERANCE: f32 = 0.006;

/// A value a gesture can hold: one number, or the XY pad's pair.
pub(crate) trait HeldValue: Copy + PartialEq + 'static {
    /// Whether `reported` is this value come back (see [`caught_up`]).
    fn caught_up(self, reported: Self) -> bool;
}

impl HeldValue for f32 {
    fn caught_up(self, reported: Self) -> bool {
        caught_up(self, reported)
    }
}

impl HeldValue for [f32; 2] {
    fn caught_up(self, reported: Self) -> bool {
        caught_up(self[0], reported[0]) && caught_up(self[1], reported[1])
    }
}

/// Whether a reported number is the held one come back, within the live
/// readings' quantum.
pub(crate) fn caught_up(held: f32, reported: f32) -> bool {
    (held - reported).abs() <= CAUGHT_UP_TOLERANCE
}

/// One control's held gesture value.
pub(crate) struct GestureHold<T: HeldValue = f32> {
    held: Signal<Option<T>>,
    /// A pointer is down on the control: the hold stays even when a
    /// snapshot happens to agree, because the next one may not.
    pressing: Signal<bool>,
    generation: Signal<u64>,
}

// Manual: a derive would demand `T: Clone`/`Copy` of the signals' bound,
// which the trait already carries.
impl<T: HeldValue> Clone for GestureHold<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: HeldValue> Copy for GestureHold<T> {}

impl<T: HeldValue> GestureHold<T> {
    /// What the control shows: the held value while it is still ahead of
    /// `reported` (the snapshot's value), else `reported`.
    pub(crate) fn shown(&self, reported: T) -> T {
        held_or_reported((self.held)(), (self.pressing)(), reported)
    }

    /// A pointer went down on the control.
    pub(crate) fn press(&mut self) {
        self.pressing.set(true);
    }

    /// The value this gesture holds, read live (an event handler can run
    /// before the render that the last [`Self::write`] scheduled).
    pub(crate) fn held(&self) -> Option<T> {
        *self.held.peek()
    }

    /// The gesture wrote `value`: hold it, and restart the fallback window.
    pub(crate) fn write(&mut self, value: T) {
        self.held.set(Some(value));
        self.arm_expiry();
    }

    /// The pointer came up (or was cancelled). The held value stays until
    /// the view catches up or the fallback window ends — unless `reported`
    /// already has: then nothing is left to bridge.
    pub(crate) fn release(&mut self, reported: T) {
        if releases(*self.held.peek(), false, reported) {
            self.clear();
            return;
        }
        self.pressing.set(false);
        self.arm_expiry();
    }

    /// Drop the hold outright (the gesture was abandoned).
    pub(crate) fn clear(&mut self) {
        self.pressing.set(false);
        self.held.set(None);
    }

    fn arm_expiry(&mut self) {
        let generation = (self.generation)() + 1;
        self.generation.set(generation);
        let mut hold = *self;
        let current = self.generation;
        spawn(async move {
            TimeoutFuture::new(GESTURE_HOLD_MS).await;
            if current() == generation {
                hold.clear();
            }
        });
    }
}

/// The hold for one control. `reported` is the value the current snapshot
/// gives the control; when it catches up with the held value after the
/// pointer is up, the hold lets go, so a later change from anywhere else
/// shows through at once.
pub(crate) fn use_gesture_hold<T: HeldValue>(reported: T) -> GestureHold<T> {
    let held = use_signal(|| None::<T>);
    let pressing = use_signal(|| false);
    let generation = use_signal(|| 0_u64);
    let mut hold = GestureHold {
        held,
        pressing,
        generation,
    };
    use_effect(use_reactive!(|reported| {
        if releases(*hold.held.peek(), *hold.pressing.peek(), reported) {
            hold.clear();
        }
    }));
    hold
}

/// The pure rule behind [`GestureHold::shown`].
fn held_or_reported<T: HeldValue>(held: Option<T>, pressing: bool, reported: T) -> T {
    match held {
        Some(held) if pressing || !held.caught_up(reported) => held,
        _ => reported,
    }
}

/// Whether the hold should let go: the pointer is up and the view now
/// reports the held value.
fn releases<T: HeldValue>(held: Option<T>, pressing: bool, reported: T) -> bool {
    held.is_some_and(|held| !pressing && held.caught_up(reported))
}

#[cfg(test)]
mod tests {
    use super::{held_or_reported, releases};

    #[test]
    fn a_stale_snapshot_never_pulls_the_value_back() {
        // Mid-drag: the hand is at 0.71, the snapshot still echoes 0.62.
        assert_eq!(held_or_reported(Some(0.71), true, 0.62), 0.71);
        // Released, the final write still on its way: still the hand's.
        assert_eq!(held_or_reported(Some(0.9), false, 0.81), 0.9);
        assert!(!releases(Some(0.9), false, 0.81));
    }

    #[test]
    fn the_hold_lets_go_once_the_view_reports_the_hand_value() {
        // The echo arrives quantized to 0.01 (0.7134 → 0.71).
        assert!(releases(Some(0.7134), false, 0.71));
        assert_eq!(held_or_reported(Some(0.7134), false, 0.71), 0.71);
    }

    #[test]
    fn a_pressed_pointer_keeps_the_hold_even_when_a_snapshot_agrees() {
        // The next snapshot may be stale again; only the release lets go.
        assert!(!releases(Some(0.5), true, 0.5));
        assert_eq!(held_or_reported(Some(0.5), true, 0.5), 0.5);
    }

    #[test]
    fn a_pair_is_caught_up_only_when_both_halves_are() {
        // The XY pad: one axis back, the other still in flight.
        assert_eq!(
            held_or_reported(Some([0.4, 0.8]), false, [0.4, 0.5]),
            [0.4, 0.8]
        );
        assert!(releases(Some([0.4, 0.8]), false, [0.401, 0.799]));
    }

    #[test]
    fn with_nothing_held_the_snapshot_shows() {
        assert_eq!(held_or_reported(None, false, 0.3_f32), 0.3);
        assert!(!releases(None, false, 0.3_f32));
    }
}
