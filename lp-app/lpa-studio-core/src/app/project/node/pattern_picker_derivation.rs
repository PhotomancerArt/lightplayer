//! Deriving the Play-mode Pattern instrument ([`crate::UiPatternPicker`]).
//!
//! The controller gathers the facts — the playlist face's entry strip, the
//! playing key, the cycle and skip values (live before authored), the failed
//! keys out of the playlist's warning, and the two channels' write targets —
//! and [`derive_pattern_picker`] turns them into names and states. The
//! gestures are the playlist's offers, built from the SAME facts
//! ([`super::playlist_offers`]), so the instrument and the verbs anyone
//! presses cannot disagree. Keeping this pure is what lets the rules below
//! be tested without a server:
//!
//! - **States**, in precedence: playing, failed, skipped, available.
//! - **Next/prev** = the adjacent authored key after the playing one,
//!   wrapping, passing over skipped and failed entries (plan PD7 — Studio
//!   picks the key and sends an activate; there is no wire command).
//! - **On/off** = the whole skip list, rewritten with one key changed.
//! - **Cycle** = the whole `PlaylistCycle`: off is `Hold`; on is a cycle at
//!   the authored step (or [`DEFAULT_STEP_SECONDS`]); the step moves along
//!   [`STEP_LADDER_SECONDS`].

use lpc_model::PlaylistCycle;

use crate::{
    OfferPath, ProjectNodeAddress, UiPanelTarget, UiPatternEntryState, UiPatternPicker,
    UiPatternPickerEntry,
};

/// The step a cycle starts at when nothing authored one.
pub const DEFAULT_STEP_SECONDS: f32 = 20.0;

/// The fade a cycle starts with when the playlist authored no fade at all.
const FALLBACK_FADE_SECONDS: f32 = 1.5;

/// The step times the instrument's shorter/longer buttons move between.
/// Discrete on purpose: a phone thumb picks "30 s", it does not drag to
/// 27.4.
pub const STEP_LADDER_SECONDS: &[f32] = &[
    5.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0, 90.0, 120.0, 180.0, 300.0, 600.0,
];

/// One entry of the set, as the controller found it.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternPickerEntryFacts {
    /// The playlist's entry key.
    pub key: u32,
    /// Its display name (the entry's authored `name`).
    pub name: String,
}

/// Everything [`derive_pattern_picker`] needs, gathered by the controller.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternPickerFacts {
    /// The playlist node, for its activates.
    pub playlist: ProjectNodeAddress,
    /// The set in authored (key) order.
    pub entries: Vec<PatternPickerEntryFacts>,
    /// The playing key.
    pub active: Option<u32>,
    /// The cycle the playlist runs now (live before authored).
    pub cycle: PlaylistCycle,
    /// The authored cycle, which is where a switched-on cycle takes its step
    /// and fade from.
    pub authored_cycle: Option<PlaylistCycle>,
    /// The playlist's authored `default_fade`, for a cycle switched on over
    /// an authored hold.
    pub default_fade: Option<f32>,
    /// The skipped keys (live before authored).
    pub skip: Vec<u32>,
    /// The keys the playlist's warning names as failed.
    pub failed: Vec<u32>,
    /// The cycle channel's write target.
    pub cycle_target: Option<UiPanelTarget>,
    /// The skip channel's write target.
    pub skip_target: Option<UiPanelTarget>,
}

/// The Pattern instrument for one playlist.
pub fn derive_pattern_picker(facts: PatternPickerFacts) -> UiPatternPicker {
    let PatternPickerFacts {
        playlist,
        entries,
        active,
        cycle,
        skip,
        failed,
        cycle_target,
        skip_target,
        ..
    } = facts;

    let entries = entries
        .into_iter()
        .map(|entry| {
            let enabled = !skip.contains(&entry.key);
            let state = if active == Some(entry.key) {
                UiPatternEntryState::Playing
            } else if failed.contains(&entry.key) {
                UiPatternEntryState::Failed
            } else if !enabled {
                UiPatternEntryState::Skipped
            } else {
                UiPatternEntryState::Available
            };
            UiPatternPickerEntry {
                key: entry.key,
                name: entry.name,
                state,
                enabled,
            }
        })
        .collect();

    UiPatternPicker {
        verbs: OfferPath::project_node(&playlist),
        entries,
        active,
        cycle,
        cycle_target,
        skip_target,
    }
}

/// Which way next/prev walks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepDirection {
    /// Towards the next authored key, wrapping to the first.
    Next,
    /// Towards the previous authored key, wrapping to the last.
    Previous,
}

/// The key next/prev plays: the nearest authored key after (or before)
/// `current` that `enabled` accepts, wrapping around the set. `None` when no
/// OTHER key qualifies — stepping onto the entry already playing is not a
/// step.
///
/// `current` need not itself be enabled (a skipped entry played by a tap
/// still has neighbours), nor even in `keys` (a key the def no longer
/// carries starts the walk from the top).
pub fn adjacent_enabled_key(
    keys: &[u32],
    current: u32,
    direction: StepDirection,
    enabled: impl Fn(u32) -> bool,
) -> Option<u32> {
    let len = keys.len();
    if len == 0 {
        return None;
    }
    let start = keys.iter().position(|key| *key == current);
    (1..=len)
        .map(|offset| {
            let index = match (start, direction) {
                (Some(start), StepDirection::Next) => (start + offset) % len,
                (Some(start), StepDirection::Previous) => (start + len - offset % len) % len,
                (None, StepDirection::Next) => offset - 1,
                (None, StepDirection::Previous) => len - offset,
            };
            keys[index]
        })
        .find(|key| *key != current && enabled(*key))
}

/// The skip list with `key` skipped (`skipped`) or included: sorted and
/// without repeats, so one set of switches always writes the same bytes.
pub fn skip_list_with(skip: &[u32], key: u32, skipped: bool) -> Vec<u32> {
    let mut next: Vec<u32> = skip.iter().copied().filter(|other| *other != key).collect();
    if skipped {
        next.push(key);
    }
    next.sort_unstable();
    next.dedup();
    next
}

/// The cycle a switched-on cycle starts as: the authored cycle when it runs,
/// else a cycle at [`DEFAULT_STEP_SECONDS`] with the authored fade (the
/// cycle's own, else the playlist's `default_fade`).
pub fn started_cycle(authored: Option<PlaylistCycle>, default_fade: Option<f32>) -> PlaylistCycle {
    if let Some(cycle) = authored
        && !cycle.is_frozen()
    {
        return cycle;
    }
    let fade_seconds = match authored {
        Some(PlaylistCycle::Cycle { fade_seconds, .. }) => fade_seconds,
        _ => default_fade.unwrap_or(FALLBACK_FADE_SECONDS),
    };
    PlaylistCycle::Cycle {
        step_seconds: DEFAULT_STEP_SECONDS,
        fade_seconds,
    }
}

/// The next rung below `step` on [`STEP_LADDER_SECONDS`].
pub fn shorter_step(step: f32) -> Option<f32> {
    STEP_LADDER_SECONDS
        .iter()
        .rev()
        .copied()
        .find(|rung| *rung < step - STEP_EPSILON)
}

/// The next rung above `step` on [`STEP_LADDER_SECONDS`].
pub fn longer_step(step: f32) -> Option<f32> {
    STEP_LADDER_SECONDS
        .iter()
        .copied()
        .find(|rung| *rung > step + STEP_EPSILON)
}

/// How far off a rung an authored step may sit and still count as on it.
const STEP_EPSILON: f32 = 0.01;

/// A step time as the instrument reads it: `20 s`, `1.5 min`, `10 min`.
pub fn format_step_seconds(step: f32) -> String {
    if step >= 60.0 {
        let minutes = step / 60.0;
        if (minutes - minutes.round()).abs() < 0.01 {
            format!("{} min", minutes.round() as u32)
        } else {
            format!("{minutes:.1} min")
        }
    } else if (step - step.round()).abs() < 0.01 {
        format!("{} s", step.round() as u32)
    } else {
        format!("{step:.1} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_and_prev_wrap_and_pass_over_disabled_keys() {
        let keys = [1, 2, 3, 4, 5];
        let enabled = |key: u32| key != 2 && key != 5;
        use StepDirection::*;
        assert_eq!(adjacent_enabled_key(&keys, 1, Next, enabled), Some(3));
        assert_eq!(
            adjacent_enabled_key(&keys, 4, Next, enabled),
            Some(1),
            "wraps past 5"
        );
        assert_eq!(
            adjacent_enabled_key(&keys, 1, Previous, enabled),
            Some(4),
            "wraps past 5"
        );
        assert_eq!(
            adjacent_enabled_key(&keys, 3, Previous, enabled),
            Some(1),
            "passes 2"
        );
    }

    #[test]
    fn a_skipped_entry_playing_by_tap_still_has_neighbours() {
        let keys = [1, 2, 3];
        let enabled = |key: u32| key != 2;
        assert_eq!(
            adjacent_enabled_key(&keys, 2, StepDirection::Next, enabled),
            Some(3)
        );
        assert_eq!(
            adjacent_enabled_key(&keys, 2, StepDirection::Previous, enabled),
            Some(1)
        );
    }

    #[test]
    fn with_nothing_else_enabled_there_is_no_step() {
        assert_eq!(
            adjacent_enabled_key(&[1, 2], 1, StepDirection::Next, |key| key == 1),
            None
        );
        assert_eq!(
            adjacent_enabled_key(&[7], 7, StepDirection::Previous, |_| true),
            None
        );
        assert_eq!(
            adjacent_enabled_key(&[], 1, StepDirection::Next, |_| true),
            None
        );
    }

    #[test]
    fn an_unknown_current_key_walks_from_the_ends() {
        let keys = [3, 4, 5];
        assert_eq!(
            adjacent_enabled_key(&keys, 9, StepDirection::Next, |_| true),
            Some(3)
        );
        assert_eq!(
            adjacent_enabled_key(&keys, 9, StepDirection::Previous, |_| true),
            Some(5)
        );
    }

    #[test]
    fn a_skip_list_is_sorted_and_whole() {
        assert_eq!(skip_list_with(&[5, 2], 3, true), vec![2, 3, 5]);
        assert_eq!(skip_list_with(&[2, 3, 5], 3, false), vec![2, 5]);
        assert_eq!(skip_list_with(&[], 1, true), vec![1]);
        assert_eq!(
            skip_list_with(&[3], 3, true),
            vec![3],
            "skipping a skipped entry writes the same list"
        );
        assert_eq!(
            skip_list_with(&[2], 3, false),
            vec![2],
            "including an included entry writes the same list"
        );
    }

    #[test]
    fn a_started_cycle_keeps_what_was_authored() {
        let authored = PlaylistCycle::Cycle {
            step_seconds: 45.0,
            fade_seconds: 3.0,
        };
        assert_eq!(started_cycle(Some(authored), Some(1.0)), authored);
        // An authored hold: the default step, the playlist's own fade.
        assert_eq!(
            started_cycle(Some(PlaylistCycle::Hold), Some(2.5)),
            PlaylistCycle::Cycle {
                step_seconds: DEFAULT_STEP_SECONDS,
                fade_seconds: 2.5
            }
        );
        // An authored frozen cycle keeps its fade.
        assert_eq!(
            started_cycle(
                Some(PlaylistCycle::Cycle {
                    step_seconds: 0.0,
                    fade_seconds: 4.0
                }),
                Some(1.0)
            ),
            PlaylistCycle::Cycle {
                step_seconds: DEFAULT_STEP_SECONDS,
                fade_seconds: 4.0
            }
        );
        assert_eq!(
            started_cycle(None, None),
            PlaylistCycle::Cycle {
                step_seconds: DEFAULT_STEP_SECONDS,
                fade_seconds: FALLBACK_FADE_SECONDS
            }
        );
    }

    #[test]
    fn the_step_ladder_moves_one_rung_and_stops_at_the_ends() {
        assert_eq!(shorter_step(20.0), Some(15.0));
        assert_eq!(longer_step(20.0), Some(30.0));
        assert_eq!(
            shorter_step(25.0),
            Some(20.0),
            "off the ladder: the rung below"
        );
        assert_eq!(
            longer_step(25.0),
            Some(30.0),
            "off the ladder: the rung above"
        );
        assert_eq!(shorter_step(5.0), None);
        assert_eq!(longer_step(600.0), None);
    }

    #[test]
    fn step_times_read_in_natural_units() {
        assert_eq!(format_step_seconds(20.0), "20 s");
        assert_eq!(format_step_seconds(7.5), "7.5 s");
        assert_eq!(format_step_seconds(60.0), "1 min");
        assert_eq!(format_step_seconds(90.0), "1.5 min");
        assert_eq!(format_step_seconds(600.0), "10 min");
    }

    #[test]
    fn states_follow_their_precedence_and_names_are_the_authored_ones() {
        let picker = derive_pattern_picker(facts(|facts| {
            facts.active = Some(2);
            facts.skip = vec![2, 3];
            facts.failed = vec![3, 4];
        }));
        let states: Vec<_> = picker
            .entries
            .iter()
            .map(|entry| (entry.key, entry.name.as_str(), entry.state, entry.enabled))
            .collect();
        use UiPatternEntryState::*;
        assert_eq!(
            states,
            vec![
                (1, "Noise", Available, true),
                (2, "Aurora", Playing, false),
                (3, "Twinkle", Failed, false),
                (4, "Scanner", Failed, true),
                (5, "Fireflies", Available, true),
            ],
            "playing beats failed beats skipped; the switch keeps its own position"
        );
    }

    #[test]
    fn the_picker_names_the_playlist_its_verbs_live_under() {
        let picker = derive_pattern_picker(facts(|_| {}));
        assert_eq!(
            picker.verbs.to_string(),
            "project/main.show/playlist.playlist"
        );
    }

    fn facts(edit: impl FnOnce(&mut PatternPickerFacts)) -> PatternPickerFacts {
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let target = |channel: &str| UiPanelTarget {
            scope,
            channel: channel.to_string(),
            engaged: false,
        };
        let mut facts = PatternPickerFacts {
            playlist: playlist_address(),
            entries: ["Noise", "Aurora", "Twinkle", "Scanner", "Fireflies"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| PatternPickerEntryFacts {
                    key: index as u32 + 1,
                    name: name.to_string(),
                })
                .collect(),
            active: Some(1),
            cycle: PlaylistCycle::Hold,
            authored_cycle: None,
            default_fade: Some(1.5),
            skip: Vec::new(),
            failed: Vec::new(),
            cycle_target: Some(target(lpc_model::PLAYLIST_CYCLE_CHANNEL)),
            skip_target: Some(target(lpc_model::PLAYLIST_SKIP_CHANNEL)),
        };
        edit(&mut facts);
        facts
    }

    fn playlist_address() -> ProjectNodeAddress {
        ProjectNodeAddress::parse("/main.show/playlist.playlist").expect("address")
    }
}
