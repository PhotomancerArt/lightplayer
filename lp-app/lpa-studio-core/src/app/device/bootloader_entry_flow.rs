//! Walking a user into bootloader mode, and telling them when they got there.
//!
//! The BOOT-button ritual is fiddly and easy to get subtly wrong — hold the
//! button *before* plugging in, not after; the strap is sampled at reset.
//! Without feedback, a failed attempt and a genuinely dead device look
//! identical, so people repeat the wrong motion and conclude the board is
//! bricked. **The confirmation is what makes the ritual learnable**, and it
//! is the reason this flow exists rather than a static list of steps.
//!
//! # Why this waits for an arrival instead of polling
//!
//! The authoritative test for bootloader mode is the esptool SYNC handshake,
//! and that handshake **reboots the device** (see
//! `docs/adr/2026-07-30-bootloader-mode-detection.md`). Polling it would
//! reboot a healthy board over and over, which is both destructive and
//! self-defeating: it could knock a working device *out* of the state the
//! user is trying to reach.
//!
//! So the flow is edge-triggered, not level-triggered. It waits for the
//! device to **re-enumerate** — the physical unplug/replug is part of the
//! ritual, so an arrival is exactly the moment worth probing — and probes
//! once, on that arrival.

use super::recovery_instructions::RecoveryInstructions;

/// Where the user is in the bootloader-entry ritual.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BootloaderEntryFlow {
    /// Showing the steps. Nothing is being probed.
    Instructing { instructions: RecoveryInstructions },
    /// The user says they have done the steps; waiting for the device to
    /// re-enumerate so there is an arrival worth probing.
    ///
    /// No probe runs in this state. A probe here would reboot whatever is
    /// currently attached, which may be the very device the user just
    /// carefully put into download mode.
    Waiting { instructions: RecoveryInstructions },
    /// A device arrived and the SYNC probe answered. This is the payoff.
    Confirmed { chip_name: Option<String> },
    /// A device arrived, was probed, and did not answer.
    ///
    /// Explicitly NOT "your device is broken": an app-mode device ignores
    /// SYNC too, so the honest reading is "that attempt did not land", and
    /// the flow returns to the steps.
    NotYet { instructions: RecoveryInstructions },
}

impl BootloaderEntryFlow {
    /// Start the flow for whatever chip Studio knows about (see
    /// [`RecoveryInstructions::for_chip`]).
    pub fn start(chip_name: Option<&str>) -> Self {
        Self::Instructing {
            instructions: RecoveryInstructions::for_chip(chip_name),
        }
    }

    /// The user pressed "I've done that" — begin waiting for an arrival.
    pub fn begin_waiting(self) -> Self {
        match self {
            Self::Instructing { instructions }
            | Self::Waiting { instructions }
            | Self::NotYet { instructions } => Self::Waiting { instructions },
            // Already confirmed: re-entering the ritual means starting over.
            Self::Confirmed { chip_name } => Self::start(chip_name.as_deref()),
        }
    }

    /// A device re-enumerated and the probe answered.
    pub fn on_probe_answered(self, chip_name: Option<String>) -> Self {
        Self::Confirmed { chip_name }
    }

    /// A device re-enumerated and the probe went unanswered.
    pub fn on_probe_unanswered(self) -> Self {
        match self {
            Self::Instructing { instructions }
            | Self::Waiting { instructions }
            | Self::NotYet { instructions } => Self::NotYet { instructions },
            Self::Confirmed { chip_name } => Self::start(chip_name.as_deref()),
        }
    }

    /// Whether an arrival should be probed right now.
    ///
    /// True **only** while waiting. This is the guard that keeps the flow
    /// from rebooting healthy devices: outside `Waiting` an arrival is just
    /// a device being plugged in, and probing it would be gratuitous.
    pub fn should_probe_on_arrival(&self) -> bool {
        matches!(self, Self::Waiting { .. })
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }

    /// The steps to show, when there are any. `None` once confirmed — the
    /// user is done and does not need the ritual again.
    pub fn instructions(&self) -> Option<&RecoveryInstructions> {
        match self {
            Self::Instructing { instructions }
            | Self::Waiting { instructions }
            | Self::NotYet { instructions } => Some(instructions),
            Self::Confirmed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_by_instructing_and_probes_nothing() {
        let flow = BootloaderEntryFlow::start(Some("ESP32-C6"));
        assert!(matches!(flow, BootloaderEntryFlow::Instructing { .. }));
        assert!(
            !flow.should_probe_on_arrival(),
            "showing steps must not probe — the user has not done anything yet"
        );
        assert!(flow.instructions().is_some());
    }

    #[test]
    fn only_the_waiting_state_probes_arrivals() {
        // The guard that stops this flow rebooting healthy boards.
        let instructing = BootloaderEntryFlow::start(None);
        assert!(!instructing.should_probe_on_arrival());

        let waiting = BootloaderEntryFlow::start(None).begin_waiting();
        assert!(waiting.should_probe_on_arrival());

        let confirmed = waiting.clone().on_probe_answered(None);
        assert!(
            !confirmed.should_probe_on_arrival(),
            "a confirmed device must not be re-probed — that would reboot it"
        );

        let not_yet = waiting.on_probe_unanswered();
        assert!(
            !not_yet.should_probe_on_arrival(),
            "after a failed attempt the user must re-arm; silently re-probing \
             every replug would reboot the board repeatedly"
        );
    }

    #[test]
    fn a_successful_probe_confirms_and_carries_the_chip() {
        let flow = BootloaderEntryFlow::start(Some("ESP32-C6"))
            .begin_waiting()
            .on_probe_answered(Some("ESP32-C6".to_string()));
        assert!(flow.is_confirmed());
        assert_eq!(
            flow,
            BootloaderEntryFlow::Confirmed {
                chip_name: Some("ESP32-C6".to_string())
            }
        );
        assert!(
            flow.instructions().is_none(),
            "a confirmed user does not need the steps again"
        );
    }

    #[test]
    fn an_unanswered_probe_returns_to_the_steps_rather_than_declaring_failure() {
        // An app-mode device ignores SYNC too, so "did not answer" is not
        // "is broken".
        let flow = BootloaderEntryFlow::start(Some("ESP32-S3"))
            .begin_waiting()
            .on_probe_unanswered();
        assert!(matches!(flow, BootloaderEntryFlow::NotYet { .. }));
        assert!(
            flow.instructions().is_some(),
            "the user needs the steps back to try again"
        );
    }

    #[test]
    fn instructions_survive_a_failed_attempt() {
        // Re-deriving them would be fine for a known chip, but for an unknown
        // one it must not silently change what the user is reading mid-ritual.
        let flow = BootloaderEntryFlow::start(None);
        let original = flow.instructions().cloned().unwrap();
        let after = flow.begin_waiting().on_probe_unanswered();
        assert_eq!(after.instructions(), Some(&original));
    }

    #[test]
    fn re_arming_from_waiting_is_idempotent() {
        let once = BootloaderEntryFlow::start(None).begin_waiting();
        let twice = once.clone().begin_waiting();
        assert_eq!(once, twice);
    }

    #[test]
    fn re_entering_after_confirmation_starts_the_ritual_over() {
        let confirmed = BootloaderEntryFlow::start(Some("ESP32-C6"))
            .begin_waiting()
            .on_probe_answered(Some("ESP32-C6".to_string()));
        let restarted = confirmed.begin_waiting();
        assert!(
            matches!(restarted, BootloaderEntryFlow::Instructing { .. }),
            "starting over shows the steps again rather than waiting blindly"
        );
        // ...and it remembers the chip, so the steps stay specific.
        assert_eq!(restarted.instructions().unwrap().subject, "ESP32-C6");
        assert!(!restarted.instructions().unwrap().is_generic);
    }
}
