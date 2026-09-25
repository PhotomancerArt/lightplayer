//! Deriving the Play-mode Pattern instrument ([`crate::UiPatternPicker`]).
//!
//! The controller gathers the facts — the playlist face's entry strip, the
//! playing key, the tour and skip values (live before authored), the failed
//! keys out of the playlist's warning, and the two channels' write targets —
//! and [`derive_pattern_picker`] turns them into names, states and ready
//! actions. Keeping this pure is what lets the rules below be tested without
//! a server:
//!
//! - **States**, in precedence: playing, failed, skipped, available.
//! - **Tap** = `PlaylistActivateOp` for every entry but the one playing.
//! - **Next/prev** = the adjacent authored key after the playing one,
//!   wrapping, passing over skipped and failed entries (plan PD7 — Studio
//!   picks the key and sends an activate; there is no wire command).
//! - **On/off** = the whole skip list, rewritten with one key flipped.
//! - **Tour** = the whole `PlaylistTour`: off is `Hold`; on is a cycle at
//!   the authored step (or [`DEFAULT_STEP_SECONDS`]); the step moves along
//!   [`STEP_LADDER_SECONDS`].

use lpc_model::{PlaylistTour, ToLpValue};

use crate::{
    ControllerId, PanelWriteOp, PlaylistActivateOp, ProjectController, ProjectNodeAddress,
    UiAction, UiPanelTarget, UiPatternEntryState, UiPatternPicker, UiPatternPickerEntry,
};

/// The step a tour starts at when nothing authored one.
pub const DEFAULT_STEP_SECONDS: f32 = 20.0;

/// The fade a tour starts with when the playlist authored no fade at all.
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
    /// The tour the playlist runs now (live before authored).
    pub tour: PlaylistTour,
    /// The authored tour, which is where a switched-on tour takes its step
    /// and fade from.
    pub authored_tour: Option<PlaylistTour>,
    /// The playlist's authored `default_fade`, for a tour switched on over
    /// an authored hold.
    pub default_fade: Option<f32>,
    /// The skipped keys (live before authored).
    pub skip: Vec<u32>,
    /// The keys the playlist's warning names as failed.
    pub failed: Vec<u32>,
    /// The tour channel's write target.
    pub tour_target: Option<UiPanelTarget>,
    /// The skip channel's write target.
    pub skip_target: Option<UiPanelTarget>,
}

/// The Pattern instrument for one playlist.
pub fn derive_pattern_picker(facts: PatternPickerFacts) -> UiPatternPicker {
    let PatternPickerFacts {
        playlist,
        entries,
        active,
        tour,
        authored_tour,
        default_fade,
        skip,
        failed,
        tour_target,
        skip_target,
    } = facts;

    let keys: Vec<u32> = entries.iter().map(|entry| entry.key).collect();
    let movable = |key: u32| !skip.contains(&key) && !failed.contains(&key);
    let step_to = |direction: StepDirection| {
        let current = active?;
        let target = adjacent_enabled_key(&keys, current, direction, movable)?;
        Some(activate_action(&playlist, target, direction.label()))
    };
    let prev = step_to(StepDirection::Previous);
    let next = step_to(StepDirection::Next);

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
            let play = (state != UiPatternEntryState::Playing)
                .then(|| activate_action(&playlist, entry.key, &format!("Play {}", entry.name)));
            let toggle = skip_target.as_ref().map(|target| {
                let verb = if enabled { "Skip" } else { "Include" };
                panel_write_action(
                    target,
                    toggled_skip(&skip, entry.key).to_lp_value(),
                    format!("{verb} {} in the tour", entry.name),
                )
            });
            UiPatternPickerEntry {
                key: entry.key,
                name: entry.name,
                state,
                enabled,
                play,
                toggle,
            }
        })
        .collect();

    let touring = !tour.is_frozen();
    let tour_toggle = tour_target.as_ref().map(|target| {
        let (value, label) = if touring {
            (PlaylistTour::Hold, "Stop the tour")
        } else {
            (
                started_tour(authored_tour, default_fade),
                "Tour the patterns",
            )
        };
        panel_write_action(target, value.to_lp_value(), label.to_string())
    });
    let step_write = |step: Option<f32>| -> Option<UiAction> {
        let (target, step) = (tour_target.as_ref()?, step?);
        let value = PlaylistTour::Cycle {
            step_seconds: step,
            fade_seconds: tour.fade_seconds(),
        };
        Some(panel_write_action(
            target,
            value.to_lp_value(),
            format!("Step every {}", format_step_seconds(step)),
        ))
    };
    let running = tour.running_step_seconds();
    let step_shorter = step_write(running.and_then(shorter_step));
    let step_longer = step_write(running.and_then(longer_step));

    UiPatternPicker {
        entries,
        active,
        tour,
        tour_target,
        skip_target,
        tour_toggle,
        step_shorter,
        step_longer,
        prev,
        next,
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

impl StepDirection {
    fn label(self) -> &'static str {
        match self {
            Self::Next => "Next pattern",
            Self::Previous => "Previous pattern",
        }
    }
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

/// The skip list with `key` switched: added when it was on, removed when it
/// was off. Sorted and without repeats, so one set of switches always writes
/// the same bytes.
pub fn toggled_skip(skip: &[u32], key: u32) -> Vec<u32> {
    let mut next: Vec<u32> = skip.iter().copied().filter(|other| *other != key).collect();
    if !skip.contains(&key) {
        next.push(key);
    }
    next.sort_unstable();
    next.dedup();
    next
}

/// The tour a switched-on tour starts as: the authored cycle when it runs,
/// else a cycle at [`DEFAULT_STEP_SECONDS`] with the authored fade (the
/// cycle's own, else the playlist's `default_fade`).
pub fn started_tour(authored: Option<PlaylistTour>, default_fade: Option<f32>) -> PlaylistTour {
    if let Some(tour) = authored
        && !tour.is_frozen()
    {
        return tour;
    }
    let fade_seconds = match authored {
        Some(PlaylistTour::Cycle { fade_seconds, .. }) => fade_seconds,
        _ => default_fade.unwrap_or(FALLBACK_FADE_SECONDS),
    };
    PlaylistTour::Cycle {
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

fn activate_action(playlist: &ProjectNodeAddress, entry: u32, label: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PlaylistActivateOp {
            node: playlist.clone(),
            entry,
        },
    )
    .with_label(label.to_string())
}

fn panel_write_action(
    target: &UiPanelTarget,
    value: lpc_model::LpValue,
    label: String,
) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PanelWriteOp {
            scope: target.scope,
            channel: target.channel.clone(),
            value,
            ttl_ms: None,
        },
    )
    .with_label(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{FromLpValue, LpValue};

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
    fn toggling_skip_is_sorted_and_whole() {
        assert_eq!(toggled_skip(&[5, 2], 3), vec![2, 3, 5]);
        assert_eq!(toggled_skip(&[2, 3, 5], 3), vec![2, 5]);
        assert_eq!(toggled_skip(&[], 1), vec![1]);
    }

    #[test]
    fn a_started_tour_keeps_what_was_authored() {
        let authored = PlaylistTour::Cycle {
            step_seconds: 45.0,
            fade_seconds: 3.0,
        };
        assert_eq!(started_tour(Some(authored), Some(1.0)), authored);
        // An authored hold: the default step, the playlist's own fade.
        assert_eq!(
            started_tour(Some(PlaylistTour::Hold), Some(2.5)),
            PlaylistTour::Cycle {
                step_seconds: DEFAULT_STEP_SECONDS,
                fade_seconds: 2.5
            }
        );
        // An authored frozen cycle keeps its fade.
        assert_eq!(
            started_tour(
                Some(PlaylistTour::Cycle {
                    step_seconds: 0.0,
                    fade_seconds: 4.0
                }),
                Some(1.0)
            ),
            PlaylistTour::Cycle {
                step_seconds: DEFAULT_STEP_SECONDS,
                fade_seconds: 4.0
            }
        );
        assert_eq!(
            started_tour(None, None),
            PlaylistTour::Cycle {
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
    fn a_tap_activates_every_entry_but_the_playing_one() {
        let picker = derive_pattern_picker(facts(|facts| facts.active = Some(2)));
        let played: Vec<Option<u32>> = picker
            .entries
            .iter()
            .map(|entry| activated(entry.play.as_ref()))
            .collect();
        assert_eq!(played, vec![Some(1), None, Some(3), Some(4), Some(5)]);
        let op = picker.entries[0]
            .play
            .as_ref()
            .and_then(|action| action.op_as::<PlaylistActivateOp>())
            .expect("activate");
        assert_eq!(
            op.node,
            playlist_address(),
            "the playlist node is addressed"
        );
    }

    #[test]
    fn next_and_prev_are_activates_of_the_neighbouring_enabled_keys() {
        let picker = derive_pattern_picker(facts(|facts| {
            facts.active = Some(1);
            facts.skip = vec![2];
            facts.failed = vec![5];
        }));
        assert_eq!(activated(picker.next.as_ref()), Some(3), "passes skipped 2");
        assert_eq!(
            activated(picker.prev.as_ref()),
            Some(4),
            "wraps, passes failed 5"
        );

        let nothing_playing = derive_pattern_picker(facts(|facts| facts.active = None));
        assert!(nothing_playing.next.is_none() && nothing_playing.prev.is_none());
    }

    #[test]
    fn an_on_off_switch_writes_the_whole_skip_list() {
        let picker = derive_pattern_picker(facts(|facts| {
            facts.active = Some(1);
            facts.skip = vec![4];
        }));
        assert_eq!(
            written(picker.entries[2].toggle.as_ref()),
            ("playlist.skip", vec![3u32, 4].to_lp_value())
        );
        assert_eq!(
            written(picker.entries[3].toggle.as_ref()).1,
            Vec::<u32>::new().to_lp_value()
        );

        let no_channel = derive_pattern_picker(facts(|facts| facts.skip_target = None));
        assert!(
            no_channel
                .entries
                .iter()
                .all(|entry| entry.toggle.is_none()),
            "no channel, no switch"
        );
    }

    #[test]
    fn the_tour_switch_and_step_write_the_whole_tour() {
        let holding = derive_pattern_picker(facts(|facts| {
            facts.authored_tour = Some(PlaylistTour::Cycle {
                step_seconds: 30.0,
                fade_seconds: 2.0,
            });
        }));
        assert!(!holding.touring());
        let (channel, value) = written(holding.tour_toggle.as_ref());
        assert_eq!(channel, "playlist.tour");
        assert_eq!(
            PlaylistTour::from_lp_value(&value).expect("tour"),
            PlaylistTour::Cycle {
                step_seconds: 30.0,
                fade_seconds: 2.0
            },
            "switching on takes the authored step"
        );
        assert!(holding.step_shorter.is_none() && holding.step_longer.is_none());

        let touring = derive_pattern_picker(facts(|facts| {
            facts.tour = PlaylistTour::Cycle {
                step_seconds: 20.0,
                fade_seconds: 1.5,
            };
        }));
        assert!(touring.touring());
        assert_eq!(
            PlaylistTour::from_lp_value(&written(touring.tour_toggle.as_ref()).1).expect("tour"),
            PlaylistTour::Hold,
            "switching off holds"
        );
        assert_eq!(
            PlaylistTour::from_lp_value(&written(touring.step_longer.as_ref()).1).expect("tour"),
            PlaylistTour::Cycle {
                step_seconds: 30.0,
                fade_seconds: 1.5
            },
            "a step keeps the fade"
        );
        assert_eq!(
            PlaylistTour::from_lp_value(&written(touring.step_shorter.as_ref()).1).expect("tour"),
            PlaylistTour::Cycle {
                step_seconds: 15.0,
                fade_seconds: 1.5
            }
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
            tour: PlaylistTour::Hold,
            authored_tour: None,
            default_fade: Some(1.5),
            skip: Vec::new(),
            failed: Vec::new(),
            tour_target: Some(target(lpc_model::PLAYLIST_TOUR_CHANNEL)),
            skip_target: Some(target(lpc_model::PLAYLIST_SKIP_CHANNEL)),
        };
        edit(&mut facts);
        facts
    }

    fn playlist_address() -> ProjectNodeAddress {
        ProjectNodeAddress::parse("/main.show/playlist.playlist").expect("address")
    }

    fn activated(action: Option<&UiAction>) -> Option<u32> {
        Some(action?.op_as::<PlaylistActivateOp>()?.entry)
    }

    fn written(action: Option<&UiAction>) -> (&'static str, LpValue) {
        let op = action
            .and_then(|action| action.op_as::<PanelWriteOp>())
            .expect("a panel write");
        let channel = match op.channel.as_str() {
            lpc_model::PLAYLIST_TOUR_CHANNEL => lpc_model::PLAYLIST_TOUR_CHANNEL,
            lpc_model::PLAYLIST_SKIP_CHANNEL => lpc_model::PLAYLIST_SKIP_CHANNEL,
            other => panic!("unexpected channel {other}"),
        };
        (channel, op.value.clone())
    }
}
