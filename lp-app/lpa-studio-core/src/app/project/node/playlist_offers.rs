//! A playlist's live verbs, as offers (M6e): what the Play-mode Pattern
//! instrument and the playlist card's entry strip press, published on the
//! playlist node itself (`project/<playlist>/<verb>`).
//!
//! | verb | takes | published when |
//! |---|---|---|
//! | `play` | `entry`, a choice of the set (the playing one disabled) | the pick is on a channel |
//! | `prev`, `next` | nothing: the neighbouring enabled entry, wrapping | the pick is on a channel, something plays and another entry is enabled |
//! | `cycle` | `cycling`, a toggle (its state is the cycle's) | the cycle is on a channel |
//! | `step-shorter`, `step-longer` | nothing: one rung of the step ladder | the cycle runs and a rung is left that way |
//! | `skip` | `entry`, a choice, and `skipped`, a toggle that is on unless the press says otherwise | the skip list is on a channel |
//! | `reset` | nothing: the pick, the cycle and the skip list back to what was authored | any of the three is held |
//!
//! **Levels.** Every one is Routine: they are panel gestures, not edits. A
//! play, next or prev is a pick — a panel write of the entry's key on the
//! playlist's `playlist.entry`, which holds and survives a reboot
//! (playlists-on-the-bus vision D1) — and the cycle, step and skip verbs
//! are whole-value panel writes on `playlist.cycle` / `playlist.skip`.
//! Nothing stages in the overlay and nothing is dirty. Turning the cycle
//! off also picks the pattern playing, so the hold survives a reboot too
//! (the cycle never writes the pick itself). `reset` lets all three go
//! (D13): one batch naming exactly those channels, since a scope reset
//! would release every shared knob with them.
//!
//! The rules (states, next/prev passing over skipped and failed entries,
//! the step ladder) are [`super::pattern_picker_derivation`]'s; this file
//! only turns the same facts into offers, so the instrument the user sees
//! and the verbs anyone presses cannot disagree.
//!
//! These verbs have controls of their own (the instrument, the strip's
//! chips), so a card's header never draws them
//! ([`crate::is_header_verb`]).

use lpc_model::{PlaylistCycle, ToLpValue};

use super::pattern_picker_derivation::{
    PatternPickerFacts, StepDirection, format_step_seconds, longer_step, shorter_step,
    skip_list_with, started_cycle,
};
use crate::{
    ControllerId, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    PanelBatchOp, PanelWriteOp, PlaylistPickOp, ProjectController, ProjectNodeAddress, UiAction,
    UiOffer, UiPanelTarget,
};

/// Play one entry of the set now.
pub const PLAYLIST_PLAY_VERB: &str = "play";
/// Play the previous enabled entry.
pub const PLAYLIST_PREV_VERB: &str = "prev";
/// Play the next enabled entry.
pub const PLAYLIST_NEXT_VERB: &str = "next";
/// Run the cycle, or hold on the playing entry.
pub const PLAYLIST_CYCLE_VERB: &str = "cycle";
/// Step the running cycle one rung shorter.
pub const PLAYLIST_STEP_SHORTER_VERB: &str = "step-shorter";
/// Step the running cycle one rung longer.
pub const PLAYLIST_STEP_LONGER_VERB: &str = "step-longer";
/// Switch an entry off (or back on) in the cycle.
pub const PLAYLIST_SKIP_VERB: &str = "skip";
/// Let the pick, the cycle and the skip list go, back to what was authored.
pub const PLAYLIST_RESET_VERB: &str = "reset";

/// The entry `play` and `skip` act on: its key in the playlist's entries
/// map, as text (`2`).
pub const PLAYLIST_ENTRY_PARAM: &str = "entry";
/// `cycle`'s toggle: on runs the cycle, off holds.
pub const PLAYLIST_CYCLING_PARAM: &str = "cycling";
/// `skip`'s toggle: on passes the entry by in the cycle, off includes it.
pub const PLAYLIST_SKIPPED_PARAM: &str = "skipped";

/// Every verb this file publishes, by its path's last segment.
pub const PLAYLIST_VERBS: [&str; 8] = [
    PLAYLIST_PLAY_VERB,
    PLAYLIST_PREV_VERB,
    PLAYLIST_NEXT_VERB,
    PLAYLIST_CYCLE_VERB,
    PLAYLIST_STEP_SHORTER_VERB,
    PLAYLIST_STEP_LONGER_VERB,
    PLAYLIST_SKIP_VERB,
    PLAYLIST_RESET_VERB,
];

/// Every offer one playlist makes, in the order the instrument reads:
/// prev, next, play, cycle, the step pair, skip, reset. None for an empty
/// set.
pub fn playlist_offers(facts: &PatternPickerFacts) -> Vec<UiOffer> {
    let at = OfferPath::project_node(&facts.playlist);
    let mut offers = Vec::new();
    if facts.entries.is_empty() {
        return offers;
    }
    if let Some(target) = &facts.entry_target {
        for direction in [StepDirection::Previous, StepDirection::Next] {
            if let Some(offer) = step_offer(&at, facts, target, direction) {
                offers.push(offer);
            }
        }
        offers.push(play_offer(&at, facts, target));
    }
    if let Some(target) = &facts.cycle_target {
        offers.push(cycle_offer(&at, facts, target));
        if let Some(step) = facts.cycle.running_step_seconds() {
            let ladder = [
                (PLAYLIST_STEP_SHORTER_VERB, shorter_step(step)),
                (PLAYLIST_STEP_LONGER_VERB, longer_step(step)),
            ];
            for (verb, rung) in ladder {
                if let Some(rung) = rung {
                    offers.push(step_time_offer(&at, facts, target, verb, rung));
                }
            }
        }
    }
    if let Some(target) = &facts.skip_target {
        offers.push(skip_offer(&at, facts, target));
    }
    if let Some(offer) = reset_offer(&at, facts) {
        offers.push(offer);
    }
    offers
}

/// Whether `verb` (an offer path's last segment) is one of a playlist's.
pub fn is_playlist_verb(verb: &str) -> bool {
    PLAYLIST_VERBS.contains(&verb)
}

/// `…/prev` or `…/next`: pick the neighbouring enabled entry, wrapping,
/// passing over skipped and failed ones ([`lpc_model::playlist_step`], the
/// engine's own rule). `None` when nothing plays or no other entry
/// qualifies.
fn step_offer(
    at: &OfferPath,
    facts: &PatternPickerFacts,
    target: &UiPanelTarget,
    direction: StepDirection,
) -> Option<UiOffer> {
    let keys: Vec<u32> = facts.entries.iter().map(|entry| entry.key).collect();
    let movable = |key: u32| !facts.skip.contains(&key) && !facts.failed.contains(&key);
    let entry = lpc_model::playlist_step(&keys, facts.stepping_from?, direction.steps(), movable)?;
    let name = entry_name(&entry_names(facts), entry);
    let (verb, label) = match direction {
        StepDirection::Previous => (PLAYLIST_PREV_VERB, "Previous pattern"),
        StepDirection::Next => (PLAYLIST_NEXT_VERB, "Next pattern"),
    };
    Some(UiOffer::new(
        at.clone().child(verb),
        "play",
        pick_action(&facts.playlist, target, entry, &format!("{label}: {name}"))
            .with_summary(format!("Play {name}, passing over patterns that are off.")),
    ))
}

/// `…/play`: pick one entry, by key. The playing entry is an option drawn
/// disabled (playing it again does nothing); a skipped entry still plays,
/// and a failed one is tried again.
fn play_offer(at: &OfferPath, facts: &PatternPickerFacts, target: &UiPanelTarget) -> UiOffer {
    let playlist = facts.playlist.clone();
    let names = entry_names(facts);
    let write_target = target.clone();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let key = picked_entry(args, &names)?;
        Ok(pick_action(
            &playlist,
            &write_target,
            key,
            &format!("Play {}", entry_name(&names, key)),
        ))
    });
    let unbound = pick_action(&facts.playlist, target, 0, "Play a pattern")
        .with_summary("Play one of the playlist's patterns now.");
    let options = facts
        .entries
        .iter()
        .map(|entry| {
            let option = OfferChoice::new(entry.key.to_string(), &entry.name);
            if facts.active == Some(entry.key) {
                option.disabled("it is playing now")
            } else if facts.failed.contains(&entry.key) {
                option.with_detail("failed to load on the device. Playing it will try again.")
            } else if facts.skip.contains(&entry.key) {
                option.with_detail("off in the cycle")
            } else {
                option
            }
        })
        .collect();
    UiOffer::with_params(
        at.clone().child(PLAYLIST_PLAY_VERB),
        "play",
        vec![OfferParam::choice(
            PLAYLIST_ENTRY_PARAM,
            "pattern",
            options,
            None,
        )],
        binder,
        unbound,
    )
}

/// `…/cycle`: run the cycle (at its authored step, or the default one) or
/// hold on the playing pattern. Left out, `cycling` keeps the cycle as it
/// is. Holding also picks the pattern playing when the pick is another, so
/// the hold survives a reboot.
fn cycle_offer(at: &OfferPath, facts: &PatternPickerFacts, target: &UiPanelTarget) -> UiOffer {
    let cycling = !facts.cycle.is_frozen();
    let running = if cycling {
        facts.cycle
    } else {
        started_cycle(facts.authored_cycle, facts.default_fade)
    };
    let target = target.clone();
    // The pick that goes with a hold: the playing entry, when it is not the
    // pick already.
    let hold_pick = match (&facts.entry_target, facts.active) {
        (Some(entry_target), Some(active)) if facts.picked != Some(active) => {
            Some(panel_write_op(entry_target, active.to_lp_value()))
        }
        _ => None,
    };
    let write = move |on: bool| {
        if on {
            return panel_write_action(
                &target,
                running.to_lp_value(),
                "Cycle the patterns".to_string(),
            );
        }
        let hold = panel_write_op(&target, PlaylistCycle::Hold.to_lp_value());
        match &hold_pick {
            Some(pick) => UiAction::from_op(
                ControllerId::new(ProjectController::NODE_ID),
                PanelBatchOp {
                    writes: vec![hold, pick.clone()],
                    clears: Vec::new(),
                },
            )
            .with_label("Stop the cycle".to_string()),
            None => panel_write_action(&target, hold.value, "Stop the cycle".to_string()),
        }
    };
    let unbound = write(!cycling);
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        Ok(write(
            args.toggle(PLAYLIST_CYCLING_PARAM).unwrap_or(cycling),
        ))
    });
    UiOffer::with_params(
        at.clone().child(PLAYLIST_CYCLE_VERB),
        "play",
        vec![OfferParam::toggle(
            PLAYLIST_CYCLING_PARAM,
            "cycle the patterns",
            cycling,
        )],
        binder,
        unbound,
    )
}

/// `…/step-shorter` / `…/step-longer`: the running cycle at `step`, its
/// fade kept.
fn step_time_offer(
    at: &OfferPath,
    facts: &PatternPickerFacts,
    target: &UiPanelTarget,
    verb: &str,
    step: f32,
) -> UiOffer {
    let value = PlaylistCycle::Cycle {
        step_seconds: step,
        fade_seconds: facts.cycle.fade_seconds(),
    };
    UiOffer::new(
        at.clone().child(verb),
        "edit",
        panel_write_action(
            target,
            value.to_lp_value(),
            format!("Step every {}", format_step_seconds(step)),
        ),
    )
}

/// `…/skip`: switch one entry off in the cycle (or, with `skipped` off,
/// back on). The write is the whole skip list with that one key changed.
fn skip_offer(at: &OfferPath, facts: &PatternPickerFacts, target: &UiPanelTarget) -> UiOffer {
    let skip = facts.skip.clone();
    let names = entry_names(facts);
    let write_target = target.clone();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let key = picked_entry(args, &names)?;
        let skipped = args.toggle(PLAYLIST_SKIPPED_PARAM).unwrap_or(true);
        let verb = if skipped { "Skip" } else { "Include" };
        Ok(panel_write_action(
            &write_target,
            skip_list_with(&skip, key, skipped).to_lp_value(),
            format!("{verb} {} in the cycle", entry_name(&names, key)),
        ))
    });
    let unbound = panel_write_action(
        target,
        facts.skip.to_lp_value(),
        "Skip a pattern in the cycle".to_string(),
    )
    .with_summary("Switch a pattern off in the cycle (or back on), so next and prev pass it by.");
    let options = facts
        .entries
        .iter()
        .map(|entry| {
            OfferChoice::new(entry.key.to_string(), &entry.name).with_detail(
                if facts.skip.contains(&entry.key) {
                    "off in the cycle now"
                } else {
                    "on in the cycle now"
                },
            )
        })
        .collect();
    UiOffer::with_params(
        at.clone().child(PLAYLIST_SKIP_VERB),
        "edit",
        vec![
            OfferParam::choice(PLAYLIST_ENTRY_PARAM, "pattern", options, None),
            OfferParam::toggle(PLAYLIST_SKIPPED_PARAM, "off in the cycle", true),
        ],
        binder,
        unbound,
    )
}

/// `…/reset`: let the pick, the cycle and the skip list go, back to what
/// was authored (vision D13). Offered while any of the three is held.
fn reset_offer(at: &OfferPath, facts: &PatternPickerFacts) -> Option<UiOffer> {
    let targets = [&facts.entry_target, &facts.cycle_target, &facts.skip_target];
    if !targets
        .iter()
        .any(|target| target.as_ref().is_some_and(|target| target.engaged))
    {
        return None;
    }
    let clears = targets
        .into_iter()
        .flatten()
        .map(|target| lpc_wire::WirePanelClearRequest::Channel {
            scope: target.scope,
            channel: target.channel.clone(),
        })
        .collect();
    Some(UiOffer::new(
        at.clone().child(PLAYLIST_RESET_VERB),
        "reset",
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            PanelBatchOp {
                writes: Vec::new(),
                clears,
            },
        )
        .with_label("Back to the authored patterns".to_string())
        .with_summary(
            "Let the picked pattern, the cycle and the patterns switched off go back to what the project says.",
        ),
    ))
}

/// The set's `(key, display name)` pairs, for a binder to name its pick.
fn entry_names(facts: &PatternPickerFacts) -> Vec<(u32, String)> {
    facts
        .entries
        .iter()
        .map(|entry| (entry.key, entry.name.clone()))
        .collect()
}

/// The display name of `key`, or `Entry <key>`.
fn entry_name(names: &[(u32, String)], key: u32) -> String {
    names
        .iter()
        .find(|(known, _)| *known == key)
        .map_or_else(|| format!("Entry {key}"), |(_, name)| name.clone())
}

/// The picked entry's key. [`UiOffer::press`] checks the value against the
/// options first, so only a press with no entry at all misses.
fn picked_entry(args: &OfferArgs, names: &[(u32, String)]) -> Result<u32, OfferArgError> {
    args.choice(PLAYLIST_ENTRY_PARAM)
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|key| names.iter().any(|(known, _)| known == key))
        .ok_or_else(|| OfferArgError::Missing {
            name: PLAYLIST_ENTRY_PARAM.to_string(),
            label: "pattern".to_string(),
        })
}

fn pick_action(
    playlist: &ProjectNodeAddress,
    target: &UiPanelTarget,
    entry: u32,
    label: &str,
) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        PlaylistPickOp {
            node: playlist.clone(),
            entry,
            scope: target.scope,
        },
    )
    .with_label(label.to_string())
}

fn panel_write_op(target: &UiPanelTarget, value: lpc_model::LpValue) -> PanelWriteOp {
    PanelWriteOp {
        scope: target.scope,
        channel: target.channel.clone(),
        value: value.into(),
        ttl_ms: None,
    }
}

fn panel_write_action(
    target: &UiPanelTarget,
    value: lpc_model::LpValue,
    label: String,
) -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        panel_write_op(target, value),
    )
    .with_label(label)
}

#[cfg(test)]
mod tests {
    use lpc_model::{FromLpValue, LpValue};

    use super::*;
    use crate::app::project::node::pattern_picker_derivation::PatternPickerEntryFacts;

    #[test]
    fn a_playlist_publishes_its_verbs_on_itself_in_instrument_order() {
        let offers = playlist_offers(&facts(|facts| {
            facts.cycle = PlaylistCycle::Cycle {
                step_seconds: 20.0,
                fade_seconds: 1.5,
            };
        }));
        assert_eq!(
            paths(&offers),
            [
                "prev",
                "next",
                "play",
                "cycle",
                "step-shorter",
                "step-longer",
                "skip"
            ]
        );
        let held = playlist_offers(&facts(|facts| {
            if let Some(target) = &mut facts.entry_target {
                target.engaged = true;
            }
        }));
        assert_eq!(
            paths(&held).last(),
            Some(&PLAYLIST_RESET_VERB),
            "reset comes last, when held"
        );
        assert!(
            offers
                .iter()
                .all(|offer| offer.path.owner().is_some_and(
                    |owner| owner.to_string() == "project/main.show/playlist.playlist"
                )),
            "every verb is the playlist node's own"
        );
        assert!(
            offers.iter().all(|offer| offer.consequence().is_routine()),
            "live pokes are Routine"
        );
        assert!(paths(&offers).iter().all(|verb| is_playlist_verb(verb)));
        assert!(
            playlist_offers(&facts(|facts| facts.entries.clear())).is_empty(),
            "an empty set offers nothing"
        );
    }

    #[test]
    fn play_picks_any_entry_but_the_playing_one() {
        let offers = playlist_offers(&facts(|facts| facts.active = Some(2)));
        let play = offer(&offers, PLAYLIST_PLAY_VERB);
        let played: Vec<Option<u32>> = (1..=5)
            .map(|key| {
                play.press(&OfferArgs::new().with(PLAYLIST_ENTRY_PARAM, key.to_string()))
                    .ok()
                    .and_then(|action| activated(&action))
            })
            .collect();
        assert_eq!(played, vec![Some(1), None, Some(3), Some(4), Some(5)]);
        let action = play
            .press(&OfferArgs::new().with(PLAYLIST_ENTRY_PARAM, "1"))
            .expect("Noise plays");
        assert_eq!(action.meta().label, "Play Noise");
        let pick = action.op_as::<PlaylistPickOp>().expect("a pick");
        assert_eq!(
            pick.node,
            playlist_address(),
            "the playlist node is addressed"
        );
        assert_eq!(
            pick.scope,
            facts(|_| {}).entry_target.expect("target").scope,
            "the write lands where the playlist reads playlist.entry"
        );
        assert!(
            !play.is_enabled(),
            "with nothing picked the verb reads as asking for a pattern"
        );
    }

    #[test]
    fn next_and_prev_pick_the_neighbouring_enabled_keys() {
        let offers = playlist_offers(&facts(|facts| {
            facts.active = Some(1);
            facts.stepping_from = Some(1);
            facts.skip = vec![2];
            facts.failed = vec![5];
        }));
        assert_eq!(
            activated(&offer(&offers, PLAYLIST_NEXT_VERB).action),
            Some(3),
            "passes skipped 2"
        );
        assert_eq!(
            activated(&offer(&offers, PLAYLIST_PREV_VERB).action),
            Some(4),
            "wraps, passes failed 5"
        );

        // A pick still on its way: the next step goes on from it, not from
        // the entry still playing.
        let in_flight = playlist_offers(&facts(|facts| facts.stepping_from = Some(3)));
        assert_eq!(
            activated(&offer(&in_flight, PLAYLIST_NEXT_VERB).action),
            Some(4)
        );

        let nothing_playing = playlist_offers(&facts(|facts| {
            facts.active = None;
            facts.stepping_from = None;
        }));
        assert!(
            !paths(&nothing_playing)
                .iter()
                .any(|verb| *verb == PLAYLIST_NEXT_VERB || *verb == PLAYLIST_PREV_VERB),
            "nowhere to step from"
        );
    }

    #[test]
    fn skip_writes_the_whole_skip_list() {
        let offers = playlist_offers(&facts(|facts| {
            facts.active = Some(1);
            facts.skip = vec![4];
        }));
        let skip = offer(&offers, PLAYLIST_SKIP_VERB);
        let pressed = skip
            .press(&OfferArgs::new().with(PLAYLIST_ENTRY_PARAM, "3"))
            .expect("skips by default");
        assert_eq!(
            written(&pressed),
            ("playlist.skip", vec![3u32, 4].to_lp_value())
        );
        assert_eq!(pressed.meta().label, "Skip Twinkle in the cycle");
        let pressed = skip
            .press(
                &OfferArgs::new()
                    .with(PLAYLIST_ENTRY_PARAM, "4")
                    .with(PLAYLIST_SKIPPED_PARAM, "false"),
            )
            .expect("includes");
        assert_eq!(written(&pressed).1, Vec::<u32>::new().to_lp_value());

        let no_channel = playlist_offers(&facts(|facts| facts.skip_target = None));
        assert!(
            !paths(&no_channel).contains(&PLAYLIST_SKIP_VERB),
            "no channel, no switch"
        );
    }

    #[test]
    fn the_cycle_switch_and_step_write_the_whole_cycle() {
        let holding = playlist_offers(&facts(|facts| {
            facts.authored_cycle = Some(PlaylistCycle::Cycle {
                step_seconds: 30.0,
                fade_seconds: 2.0,
            });
        }));
        let cycle = offer(&holding, PLAYLIST_CYCLE_VERB);
        let on = cycle
            .press(&OfferArgs::new().with(PLAYLIST_CYCLING_PARAM, "true"))
            .expect("switch on");
        let (channel, value) = written(&on);
        assert_eq!(channel, "playlist.cycle");
        assert_eq!(
            PlaylistCycle::from_lp_value(&value).expect("cycle"),
            PlaylistCycle::Cycle {
                step_seconds: 30.0,
                fade_seconds: 2.0
            },
            "switching on takes the authored step"
        );
        assert!(
            !paths(&holding).iter().any(|verb| verb.starts_with("step-")),
            "a held cycle has no step to move"
        );

        let cycling = playlist_offers(&facts(|facts| {
            facts.cycle = PlaylistCycle::Cycle {
                step_seconds: 20.0,
                fade_seconds: 1.5,
            };
        }));
        let off = offer(&cycling, PLAYLIST_CYCLE_VERB)
            .press(&OfferArgs::new().with(PLAYLIST_CYCLING_PARAM, "false"))
            .expect("switch off");
        assert_eq!(
            PlaylistCycle::from_lp_value(&written(&off).1).expect("cycle"),
            PlaylistCycle::Hold,
            "switching off holds; the playing entry is the pick already"
        );

        // The cycle walked on from the pick: holding also picks the entry
        // playing, so the hold survives a reboot.
        let walked_on = playlist_offers(&facts(|facts| {
            facts.cycle = PlaylistCycle::Cycle {
                step_seconds: 20.0,
                fade_seconds: 1.5,
            };
            facts.active = Some(3);
        }));
        let off = offer(&walked_on, PLAYLIST_CYCLE_VERB)
            .press(&OfferArgs::new().with(PLAYLIST_CYCLING_PARAM, "false"))
            .expect("switch off");
        let batch = off.op_as::<PanelBatchOp>().expect("a hold and a pick");
        let channels: Vec<(&str, &LpValue)> = batch
            .writes
            .iter()
            .map(|write| (write.channel.as_str(), &write.value))
            .collect();
        assert_eq!(
            channels,
            [
                ("playlist.cycle", &PlaylistCycle::Hold.to_lp_value()),
                ("playlist.entry", &LpValue::U32(3)),
            ]
        );
        assert_eq!(
            PlaylistCycle::from_lp_value(
                &written(&offer(&cycling, PLAYLIST_STEP_LONGER_VERB).action).1
            )
            .expect("cycle"),
            PlaylistCycle::Cycle {
                step_seconds: 30.0,
                fade_seconds: 1.5
            },
            "a step keeps the fade"
        );
        assert_eq!(
            PlaylistCycle::from_lp_value(
                &written(&offer(&cycling, PLAYLIST_STEP_SHORTER_VERB).action).1
            )
            .expect("cycle"),
            PlaylistCycle::Cycle {
                step_seconds: 15.0,
                fade_seconds: 1.5
            }
        );

        let shortest = playlist_offers(&facts(|facts| {
            facts.cycle = PlaylistCycle::Cycle {
                step_seconds: 5.0,
                fade_seconds: 1.5,
            };
        }));
        assert!(
            !paths(&shortest).contains(&PLAYLIST_STEP_SHORTER_VERB),
            "the ladder's end offers no shorter step"
        );
    }

    #[test]
    fn reset_lets_the_pick_the_cycle_and_the_skip_list_go() {
        assert!(
            !paths(&playlist_offers(&facts(|_| {}))).contains(&PLAYLIST_RESET_VERB),
            "nothing held, nothing to reset"
        );
        let offers = playlist_offers(&facts(|facts| {
            if let Some(target) = &mut facts.skip_target {
                target.engaged = true;
            }
        }));
        let action = &offer(&offers, PLAYLIST_RESET_VERB).action;
        let batch = action.op_as::<PanelBatchOp>().expect("a batch of clears");
        let channels: Vec<String> = batch
            .clears
            .iter()
            .map(|clear| match clear {
                lpc_wire::WirePanelClearRequest::Channel { channel, .. } => channel.clone(),
                other => panic!("only channel clears: {other:?}"),
            })
            .collect();
        assert_eq!(
            channels,
            ["playlist.entry", "playlist.cycle", "playlist.skip"]
        );
        assert!(batch.writes.is_empty());
    }

    fn paths(offers: &[UiOffer]) -> Vec<&str> {
        offers
            .iter()
            .map(|offer| offer.path.last().unwrap_or_default())
            .collect()
    }

    fn offer<'a>(offers: &'a [UiOffer], verb: &str) -> &'a UiOffer {
        offers
            .iter()
            .find(|offer| offer.path.last() == Some(verb))
            .unwrap_or_else(|| panic!("`{verb}` is offered"))
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
            entry_target: Some(target(lpc_model::PLAYLIST_ENTRY_CHANNEL)),
            picked: Some(1),
            stepping_from: Some(1),
        };
        edit(&mut facts);
        facts
    }

    fn playlist_address() -> ProjectNodeAddress {
        ProjectNodeAddress::parse("/main.show/playlist.playlist").expect("address")
    }

    fn activated(action: &UiAction) -> Option<u32> {
        Some(action.op_as::<PlaylistPickOp>()?.entry)
    }

    fn written(action: &UiAction) -> (&'static str, LpValue) {
        let op = action.op_as::<PanelWriteOp>().expect("a panel write");
        let channel = match op.channel.as_str() {
            lpc_model::PLAYLIST_CYCLE_CHANNEL => lpc_model::PLAYLIST_CYCLE_CHANNEL,
            lpc_model::PLAYLIST_SKIP_CHANNEL => lpc_model::PLAYLIST_SKIP_CHANNEL,
            other => panic!("unexpected channel {other}"),
        };
        (
            channel,
            op.value.as_value().expect("a whole-value write").clone(),
        )
    }
}
