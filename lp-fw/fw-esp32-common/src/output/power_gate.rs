//! Switched power rails: the behavior half of `HwManifest::power_gates`.
//!
//! Some boards put the LED supply behind a GPIO — the dig2go's GPIO12 cuts
//! the strip's power entirely. The manifest descriptor says "assert this pin
//! or these outputs are dead" and carries the constants; everything that
//! *acts* on it lives here, driven by [`Esp32OutputProvider`] and by a
//! chip-side [`PowerGatePin`].
//!
//! See `docs/adr/2026-08-08-switched-power-rail-mechanism.md`.
//!
//! [`Esp32OutputProvider`]: super::provider::Esp32OutputProvider

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_hardware::{HwAddress, HwGateLevel, HwPowerGate};

/// The chip-side half of a gate: one pin, driven to a **physical** level.
///
/// Physical rather than logical because `active_level` is metadata and this
/// module is the only place that reads it. An implementation free to invert
/// on its own would make the descriptor's polarity mean different things on
/// different chips, which is exactly what putting polarity in metadata was
/// meant to prevent. `open_drain` never appears here either: it is a
/// construction-time property of the pad.
///
/// Implementations must be able to drive the pin from construction onward —
/// [`PowerGateController::new`] drives every pin to its inactive level
/// immediately, and a strap-class gate pin (the dig2go's GPIO12 is MTDI, the
/// flash-voltage strap) must never idle high while the provider is deciding.
pub trait PowerGatePin {
    fn set_level(&mut self, high: bool);
}

/// Monotonic microseconds. Injected as a closure rather than read from a HAL:
/// fw-esp32-common builds under three chip toolchains and none of its time may
/// be ambient (`docs/adr/2026-07-06-sans-io-core.md`), and the settle window
/// below is only testable if the clock can be advanced by hand.
pub type NowMicros = Box<dyn Fn() -> u64>;

/// True when the frame carries no light at all, stopping at the first lit
/// value.
///
/// The provider cannot see brightness — it is applied upstream, so brightness
/// zero arrives here as zeros — which is why "all black" is the trigger rather
/// than an intent flag. The early exit is what makes that affordable: a lit
/// frame pays one comparison, and only a genuinely black frame pays the full
/// walk, which is the frame whose rail we are about to switch off anyway.
pub fn is_all_black(data: &[u16]) -> bool {
    data.iter().all(|value| *value == 0)
}

/// One gate: its descriptor, its pin, and the state the transitions need.
struct Gate {
    descriptor: HwPowerGate,
    pin: Box<dyn PowerGatePin>,
    asserted: bool,
    /// Monotonic µs of the most recent lit frame on a channel this gate
    /// feeds. Meaningless while the rail is down; the deassert debounce runs
    /// from it while it is up.
    last_lit_us: u64,
}

impl Gate {
    /// The physical level that asserts this gate.
    fn active_high(&self) -> bool {
        matches!(self.descriptor.active_level(), HwGateLevel::High)
    }

    fn feeds(&self, address: Option<&HwAddress>) -> bool {
        // Empty `feeds` gates every output. A channel whose address could not
        // be resolved is held only by such a gate: guessing membership from a
        // missing address would either strand a rail down or hold it up on a
        // channel that does not use it.
        match self.descriptor.feeds() {
            [] => true,
            feeds => address.is_some_and(|address| feeds.iter().any(|fed| fed == address)),
        }
    }
}

/// Every gate a board declares, plus the clock the transitions are timed
/// against.
///
/// Channels are addressed by a **mask** of gate indices rather than by
/// handle: the provider resolves membership once at `open`, so the write path
/// never walks descriptors or compares addresses. A `u32` mask caps the board
/// at 32 gates, against a family whose widest declares three.
pub struct PowerGateController {
    gates: Vec<Gate>,
    now_us: NowMicros,
}

impl PowerGateController {
    /// Build the controller and leave every rail **off**.
    ///
    /// Off at construction is not tidiness: on the dig2go the gate is GPIO12 =
    /// MTDI, the flash-voltage strap, which must be low at boot or VDD_SDIO
    /// selects 1.8 V and the board does not come up. Inactive is also the only
    /// state that is correct before any frame has been seen.
    pub fn new(
        now_us: NowMicros,
        gates: impl IntoIterator<Item = (HwPowerGate, Box<dyn PowerGatePin>)>,
    ) -> Self {
        let gates = gates
            .into_iter()
            .map(|(descriptor, pin)| {
                let mut gate = Gate {
                    descriptor,
                    pin,
                    asserted: false,
                    last_lit_us: 0,
                };
                let inactive = !gate.active_high();
                gate.pin.set_level(inactive);
                gate
            })
            .collect();
        Self { gates, now_us }
    }

    /// Which gates hold up the channel at `address` (bit `n` = gate `n`).
    ///
    /// Matched against the **endpoint's own address**, the only address the
    /// provider can resolve for an open channel: which `/rmt/ws281xK` slot
    /// carries a wire is decided per transmission on the classic, so it is not
    /// a stable identity to scope a rail by. A profile that names `feeds`
    /// therefore names endpoint addresses.
    pub fn mask_for(&self, address: Option<&HwAddress>) -> u32 {
        let mut mask = 0;
        for (index, gate) in self.gates.iter().enumerate().take(u32::BITS as usize) {
            if gate.feeds(address) {
                mask |= 1 << index;
            }
        }
        mask
    }

    /// Account for one frame about to be staged on the channels in `mask`,
    /// energising and settling the rail if the frame is lit and the rail is
    /// down.
    ///
    /// Returns after the settle window has fully elapsed, so the caller may
    /// transmit immediately. That wait is explicit rather than implied by call
    /// order: clocking WS281x data into an unpowered strip phantom-powers the
    /// first controller through its data-pin protection diode, and our pusher
    /// runs on the APP core with waves queued, so "we asserted first" proves
    /// nothing about what the wire is doing.
    pub fn on_frame(&mut self, mask: u32, all_black: bool) {
        if all_black {
            return;
        }
        let now = (self.now_us)();
        let mut settled_at: Option<u64> = None;
        for gate in Self::selected_mut(&mut self.gates, mask) {
            gate.last_lit_us = now;
            if gate.asserted {
                continue;
            }
            let active = gate.active_high();
            gate.pin.set_level(active);
            gate.asserted = true;
            let deadline = now + u64::from(gate.descriptor.settle_ms()) * 1_000;
            settled_at = Some(settled_at.map_or(deadline, |current: u64| current.max(deadline)));
        }
        // Steady state — every rail this frame needs was already up — leaves
        // here having spent one clock read and nothing else.
        let Some(settled_at) = settled_at else {
            return;
        };

        let mut settled = now;
        while settled < settled_at {
            settled = (self.now_us)();
        }
        // Restamp: the debounce runs from the frame that actually reaches the
        // wire, not from the decision to energise, so the first window after
        // an assert is not short by the settle it just spent.
        for gate in Self::selected_mut(&mut self.gates, mask) {
            gate.last_lit_us = settled;
        }
    }

    /// Gates whose trailing all-black debounce has expired — the deassert
    /// *candidates*.
    ///
    /// Split from [`Self::deassert`] so the caller can drain the wires those
    /// gates feed in between. Cutting power with a frame in flight is the
    /// failure this split exists to prevent.
    pub fn expired(&self) -> u32 {
        let now = (self.now_us)();
        let mut mask = 0;
        for (index, gate) in self.gates.iter().enumerate().take(u32::BITS as usize) {
            if gate.asserted
                && now.saturating_sub(gate.last_lit_us)
                    >= u64::from(gate.descriptor.off_debounce_ms()) * 1_000
            {
                mask |= 1 << index;
            }
        }
        mask
    }

    /// Drop the rails in `mask`. Call only once their wires are drained.
    pub fn deassert(&mut self, mask: u32) {
        for gate in Self::selected_mut(&mut self.gates, mask) {
            if !gate.asserted {
                continue;
            }
            let inactive = !gate.active_high();
            gate.pin.set_level(inactive);
            gate.asserted = false;
        }
    }

    fn selected_mut(gates: &mut [Gate], mask: u32) -> impl Iterator<Item = &mut Gate> {
        gates
            .iter_mut()
            .enumerate()
            .filter(move |(index, _)| mask & (1 << index) != 0)
            .map(|(_, gate)| gate)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::is_all_black;

    #[test]
    fn a_lit_first_value_is_not_all_black() {
        let mut frame = vec![0u16; 300];
        frame[0] = 1;
        assert!(!is_all_black(&frame));
    }

    #[test]
    fn a_lit_last_value_is_not_all_black() {
        let mut frame = vec![0u16; 300];
        *frame.last_mut().expect("non-empty") = 1;
        assert!(!is_all_black(&frame));
    }

    #[test]
    fn an_all_zero_frame_is_all_black() {
        assert!(is_all_black(&vec![0u16; 300]));
    }

    /// A frame with no pixels at all cannot hold a rail up.
    #[test]
    fn an_empty_frame_is_all_black() {
        assert!(is_all_black(&[]));
    }
}
