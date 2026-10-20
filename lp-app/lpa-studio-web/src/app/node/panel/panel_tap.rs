//! A tap on a panel control: a press that moves less than [`TAP_SLOP_PX`]
//! before its release. It writes nothing and opens the control's details
//! (the board card's cells and strips, where the whole cell is the control's
//! target and a hand that only touched it must not engage a channel).
//!
//! The Speed knob's tap waits out [`DOUBLE_CLICK_WINDOW_MS`] first, because
//! its double-click returns it to 1×: [`TapGate`] arms on the tap and any
//! later press (the double-click's second one) disarms it.

/// How far (CSS px) a press may wander before it is a drag. Measured from
/// the press point; a drag writes nothing until it has gone this far.
pub(crate) const TAP_SLOP_PX: f64 = 4.0;

/// How long the Speed knob's tap waits for a second press before it opens
/// its details: about a desktop's double-click interval.
pub(crate) const DOUBLE_CLICK_WINDOW_MS: u32 = 300;

/// Whether the pointer has gone past the tap slop since the press at
/// `from`: from here on the gesture is a drag.
pub(crate) fn past_tap_slop(from: (f64, f64), to: (f64, f64)) -> bool {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    dx * dx + dy * dy >= TAP_SLOP_PX * TAP_SLOP_PX
}

/// What a control's release does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelRelease {
    /// The drag wrote: land its last value exactly (the throttle may have
    /// swallowed the last moves).
    Flush,
    /// The press never left the slop and wrote nothing: open the details.
    Tap,
    /// Nothing to do: no press under way, or a drag that never wrote.
    Nothing,
}

/// The release of a press: `pressed` while one is under way, `past_slop`
/// once it moved past the tap slop (always, on a control that has no tap),
/// `wrote` once its drag wrote a value.
pub(crate) fn panel_release(pressed: bool, past_slop: bool, wrote: bool) -> PanelRelease {
    match (pressed, past_slop, wrote) {
        (false, _, _) => PanelRelease::Nothing,
        (true, _, true) => PanelRelease::Flush,
        (true, false, false) => PanelRelease::Tap,
        (true, true, false) => PanelRelease::Nothing,
    }
}

/// A tap that fires only if nothing presses again within the double-click
/// window. [`TapGate::arm`] hands out a token; a later press or double-click
/// ([`TapGate::cancel`]) makes every earlier token stale; the window's timer
/// fires the tap only while its token is still current.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TapGate {
    generation: u32,
    armed: bool,
}

impl TapGate {
    /// A tap landed: wait out the window under the returned token.
    pub(crate) fn arm(&mut self) -> u32 {
        self.generation = self.generation.wrapping_add(1);
        self.armed = true;
        self.generation
    }

    /// Another press (or the double-click itself): the waiting tap is off.
    pub(crate) fn cancel(&mut self) {
        self.armed = false;
    }

    /// The window's timer: whether the tap armed under `token` still fires.
    /// It fires once.
    pub(crate) fn fire(&mut self, token: u32) -> bool {
        let fires = self.armed && self.generation == token;
        if fires {
            self.armed = false;
        }
        fires
    }
}

#[cfg(test)]
mod tests {
    use super::{PanelRelease, TAP_SLOP_PX, TapGate, panel_release, past_tap_slop};

    /// Under the slop a press is still a tap, in any direction; at the slop
    /// it is a drag.
    #[test]
    fn the_slop_decides_tap_or_drag() {
        assert!(!past_tap_slop((10.0, 10.0), (10.0, 10.0)));
        assert!(
            !past_tap_slop((10.0, 10.0), (12.0, 12.0)),
            "2.8 px off is a tap"
        );
        assert!(!past_tap_slop(
            (10.0, 10.0),
            (10.0, 10.0 - TAP_SLOP_PX + 0.5)
        ));
        assert!(past_tap_slop((10.0, 10.0), (10.0, 10.0 - TAP_SLOP_PX)));
        assert!(past_tap_slop((10.0, 10.0), (10.0 + TAP_SLOP_PX, 10.0)));
        assert!(
            past_tap_slop((10.0, 10.0), (13.0, 13.0)),
            "4.2 px off is a drag"
        );
    }

    /// A press that never left the slop is a tap, and a tap writes nothing:
    /// its release opens the details and never flushes a value.
    #[test]
    fn a_tap_writes_nothing() {
        assert_eq!(panel_release(true, false, false), PanelRelease::Tap);
        assert_eq!(panel_release(true, true, true), PanelRelease::Flush);
        // Past the slop with no write yet (a drag that came back): nothing.
        assert_eq!(panel_release(true, true, false), PanelRelease::Nothing);
        // A release with no press under way (a missed press): nothing.
        assert_eq!(panel_release(false, false, false), PanelRelease::Nothing);
    }

    /// The Speed knob's tap fires once its window passes with no second
    /// press, and only once.
    #[test]
    fn an_armed_tap_fires_after_the_window() {
        let mut gate = TapGate::default();
        let token = gate.arm();
        assert!(gate.fire(token));
        assert!(!gate.fire(token), "a tap fires once");
    }

    /// A second press inside the window (a double-click) cancels the tap:
    /// the double-click returns the knob to 1× and opens nothing.
    #[test]
    fn a_double_click_cancels_the_waiting_tap() {
        let mut gate = TapGate::default();
        let token = gate.arm();
        gate.cancel();
        assert!(!gate.fire(token));
    }

    /// A newer tap makes the older one's timer stale: only the last tap's
    /// window opens the details.
    #[test]
    fn only_the_latest_tap_fires() {
        let mut gate = TapGate::default();
        let first = gate.arm();
        let second = gate.arm();
        assert!(!gate.fire(first));
        assert!(gate.fire(second));
    }
}
