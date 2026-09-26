//! The Play-mode **Pattern** instrument: pick what a playlist plays.
//!
//! Multi-pattern vision D15/D21. A playlist holding a set of patterns shows,
//! between the shared knobs and the playing pattern's own knobs:
//!
//! - the set's display names in authored order, the playing one marked —
//!   tap one to play it (a dormant entry loads);
//! - a cycle switch with its step time;
//! - a per-pattern on/off, where off means "skip it in the cycle";
//! - next and previous.
//!
//! Names only; thumbnails are vision Q1. Every gesture arrives here as a
//! ready [`UiAction`] the derivation built
//! (`app/project/node/pattern_picker_derivation.rs`), so the widget renders
//! and dispatches and decides nothing: a tap is a `PlaylistActivateOp`, the
//! cycle and the on/off switches are whole-value panel writes on the
//! playlist's `playlist.cycle` / `playlist.skip` channels, and next/prev are
//! activates of a key the derivation already chose (plan PD7 — no wire
//! command for them).

use crate::{UiAction, UiPanelTarget};

/// Everything the Pattern instrument shows and every gesture it can make.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPatternPicker {
    /// The set, in authored (key) order.
    pub entries: Vec<UiPatternPickerEntry>,
    /// The entry playing now (`PlaylistState.active_entry`), `None` before
    /// the first status read.
    pub active: Option<u32>,
    /// The cycle the playlist runs: the channel's value when something
    /// writes it (a panel write's local echo included), else the authored
    /// default — the live-before-authored rule a palette swatch follows
    /// (`UiPanelControl::shown_palette`).
    pub cycle: lpc_model::PlaylistCycle,
    /// Where a cycle write lands, and whether the panel holds it now.
    /// `None` when the playlist's cycle is not on a channel (the gestures
    /// are then absent too).
    pub cycle_target: Option<UiPanelTarget>,
    /// Where an on/off write lands, and whether the panel holds it now.
    pub skip_target: Option<UiPanelTarget>,
    /// Turn the cycle on (a cycle at the remembered step) or off (hold).
    pub cycle_toggle: Option<UiAction>,
    /// A shorter step, while cycling and not at the shortest step.
    pub step_shorter: Option<UiAction>,
    /// A longer step, while cycling and not at the longest step.
    pub step_longer: Option<UiAction>,
    /// Play the previous enabled entry (wrapping), `None` when there is no
    /// other enabled entry to go to.
    pub prev: Option<UiAction>,
    /// Play the next enabled entry (wrapping).
    pub next: Option<UiAction>,
}

impl UiPatternPicker {
    /// Whether the cycle is walking the set now (a cycle that is not frozen).
    pub fn cycling(&self) -> bool {
        !self.cycle.is_frozen()
    }

    /// The step of a running cycle, in seconds.
    pub fn step_seconds(&self) -> Option<f32> {
        self.cycle.running_step_seconds()
    }

    /// The entry playing now, when it is one of the set.
    pub fn playing(&self) -> Option<&UiPatternPickerEntry> {
        self.entries
            .iter()
            .find(|entry| entry.state == UiPatternEntryState::Playing)
    }
}

/// One pattern in the set.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPatternPickerEntry {
    /// The playlist's entry key.
    pub key: u32,
    /// The entry's display name: its authored `name` read the way its node
    /// card reads (`noise_soft` → `Noise soft`; vision Q9 — node names
    /// cannot carry spaces or dashes), else `Entry <key>`.
    pub name: String,
    /// What the entry is doing, for how it reads.
    pub state: UiPatternEntryState,
    /// The on/off switch's position: `false` = skipped in the cycle. Carried
    /// apart from [`Self::state`] because the playing entry (or a failed
    /// one) still has a switch position of its own.
    pub enabled: bool,
    /// Tap to play: `PlaylistActivateOp`. `None` for the entry already
    /// playing. A skipped entry can still be tapped; a failed one is tried
    /// again.
    pub play: Option<UiAction>,
    /// Flip on/off: a whole-list panel write on the skip channel.
    pub toggle: Option<UiAction>,
}

/// What one entry is doing, in the precedence the instrument shows it:
/// playing beats failed beats skipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiPatternEntryState {
    /// Loaded and on the lamps.
    Playing,
    /// Dormant and able to play.
    Available,
    /// Switched off: the cycle, next/prev and triggers pass it by.
    Skipped,
    /// Its load or compile failed on the device; the cycle passes it by until
    /// it is edited or the project reloads. A tap tries it again.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(cycle: lpc_model::PlaylistCycle, states: &[UiPatternEntryState]) -> UiPatternPicker {
        UiPatternPicker {
            entries: states
                .iter()
                .enumerate()
                .map(|(index, state)| UiPatternPickerEntry {
                    key: index as u32 + 1,
                    name: format!("p{index}"),
                    state: *state,
                    enabled: *state != UiPatternEntryState::Skipped,
                    play: None,
                    toggle: None,
                })
                .collect(),
            active: None,
            cycle,
            cycle_target: None,
            skip_target: None,
            cycle_toggle: None,
            step_shorter: None,
            step_longer: None,
            prev: None,
            next: None,
        }
    }

    #[test]
    fn a_frozen_cycle_is_not_cycling() {
        use lpc_model::PlaylistCycle;
        let running = PlaylistCycle::Cycle {
            step_seconds: 20.0,
            fade_seconds: 1.5,
        };
        assert!(picker(running, &[]).cycling());
        assert_eq!(picker(running, &[]).step_seconds(), Some(20.0));
        let frozen = PlaylistCycle::Cycle {
            step_seconds: 0.0,
            fade_seconds: 1.5,
        };
        assert!(!picker(frozen, &[]).cycling());
        assert!(!picker(PlaylistCycle::Hold, &[]).cycling());
    }

    #[test]
    fn the_playing_entry_is_found_by_state() {
        use UiPatternEntryState::*;
        let picker = picker(
            lpc_model::PlaylistCycle::Hold,
            &[Available, Playing, Skipped],
        );
        assert_eq!(picker.playing().map(|entry| entry.key), Some(2));
    }
}
