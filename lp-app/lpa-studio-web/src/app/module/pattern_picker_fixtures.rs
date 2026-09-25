//! Story fixtures for the Play-mode Pattern instrument.
//!
//! Honest to the device (director ruling 3): the device keeps ONE entry
//! loaded, so a picker knows every other entry by its def alone — a key and
//! an authored name — and knows the playing one's knobs because that entry
//! is the one loaded. The instrument itself is built by core's own
//! [`derive_pattern_picker`] from those facts, so a story shows exactly
//! the names, states and actions the real derivation would.

use lpa_studio_core::app::project::node::pattern_picker_derivation::{
    PatternPickerEntryFacts, PatternPickerFacts, derive_pattern_picker,
};
use lpa_studio_core::{
    ProjectNodeAddress, UiAction, UiPanelControl, UiPanelControlState, UiPanelControlView,
    UiPanelEmit, UiPanelGroup, UiPanelTarget, UiPanelWidget, UiSlotFieldState, UiSlotValue,
};
use lpc_model::{FromLpValue, PlaylistTour};

use super::module_fixtures::{
    PIECE_SCOPE, PLAYING_PATTERN_SCOPE, at_default, fader, knob, scope_target, swatch,
};

/// The choker tryout's five patterns (PR #809), by their authored names.
pub(crate) const CHOKER_SET: &[&str] = &["noise_soft", "aurora", "twinkle", "scanner", "fireflies"];

/// Twenty-five patterns — the plan's target set size (AC3).
pub(crate) const TWENTY_FIVE: &[&str] = &[
    "noise_soft",
    "aurora",
    "twinkle",
    "scanner",
    "fireflies",
    "comet",
    "fire2012",
    "meteor",
    "plasma",
    "ripple",
    "sinelon",
    "juggle",
    "bpm_pulse",
    "confetti",
    "rainbow_march",
    "lava_lamp",
    "ocean_swell",
    "candle_flicker",
    "starfield",
    "heartbeat",
    "breathing",
    "color_waves",
    "sparkle_rain",
    "pacifica",
    "twinkle_fox",
];

/// Facts for a set of `names` (keys 1…n), playing `active`.
pub(crate) fn set_facts(names: &[&str], active: u32) -> PatternPickerFacts {
    let target = |channel: &str| UiPanelTarget {
        scope: scope_target(PIECE_SCOPE),
        channel: channel.to_string(),
        engaged: false,
    };
    PatternPickerFacts {
        playlist: ProjectNodeAddress::parse("/playful.module/playlist.playlist")
            .expect("valid story playlist address"),
        entries: names
            .iter()
            .enumerate()
            .map(|(index, name)| PatternPickerEntryFacts {
                key: index as u32 + 1,
                name: human(name),
            })
            .collect(),
        active: Some(active),
        tour: PlaylistTour::Hold,
        authored_tour: None,
        default_fade: Some(1.5),
        skip: Vec::new(),
        failed: Vec::new(),
        tour_target: Some(target(lpc_model::PLAYLIST_TOUR_CHANNEL)),
        skip_target: Some(target(lpc_model::PLAYLIST_SKIP_CHANNEL)),
    }
}

/// A running tour, as authored in the playlist file.
pub(crate) fn authored_tour(step_seconds: f32) -> PlaylistTour {
    PlaylistTour::Cycle {
        step_seconds,
        fade_seconds: 1.5,
    }
}

/// Mark the panel as holding the tour (a Play-mode write), so the tour
/// controls wear the engaged gold.
pub(crate) fn tour_held(mut facts: PatternPickerFacts) -> PatternPickerFacts {
    if let Some(target) = facts.tour_target.as_mut() {
        target.engaged = true;
    }
    facts
}

/// Mark the panel as holding the on/off switches.
pub(crate) fn skip_held(mut facts: PatternPickerFacts) -> PatternPickerFacts {
    if let Some(target) = facts.skip_target.as_mut() {
        target.engaged = true;
    }
    facts
}

/// The whole Play-mode panel for a piece whose playlist the facts
/// describe: shared knobs, then the Pattern instrument, then the playing
/// pattern's own knobs.
pub(crate) fn piece_panel(facts: &PatternPickerFacts) -> UiPanelGroup {
    let playing = facts
        .entries
        .iter()
        .find(|entry| Some(entry.key) == facts.active)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    UiPanelGroup::new("Playful Choker", PIECE_SCOPE)
        .with_target(scope_target(PIECE_SCOPE))
        .with_controls(shared_controls())
        .with_groups(vec![pattern_group(facts), pattern_knobs(&playing)])
}

/// The Pattern group, built by core's derivation.
pub(crate) fn pattern_group(facts: &PatternPickerFacts) -> UiPanelGroup {
    let picker = derive_pattern_picker(facts.clone());
    let held =
        |target: &Option<UiPanelTarget>| target.as_ref().is_some_and(|target| target.engaged);
    let state = if held(&facts.tour_target) || held(&facts.skip_target) {
        UiPanelControlState::Engaged
    } else {
        UiPanelControlState::ReadDefault
    };
    let playing = picker
        .playing()
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let control = UiPanelControl {
        label: "Pattern".to_string(),
        address: None,
        value: UiSlotValue::string(playing),
        panel_target: picker.tour_target.clone(),
        widget: UiPanelWidget::PatternPicker { picker },
        emit: UiPanelEmit::Value,
        live_value: None,
        live_gradient: None,
        wires: Vec::new(),
        unit: None,
        state: UiSlotFieldState::editable(),
        aspects: Vec::new(),
    };
    UiPanelGroup::new("Pattern", "/playful.module/playlist.playlist").with_controls(vec![
        UiPanelControlView::new(lpc_model::PLAYLIST_TOUR_CHANNEL, control)
            .with_state(state, None::<String>),
    ])
}

/// Apply one of the picker's own actions to the facts, the way the device
/// answers it: an activate plays that entry, a tour or skip write replaces
/// the value and the panel holds it. The walkable story's whole engine.
pub(crate) fn apply_picker_action(facts: &mut PatternPickerFacts, action: &UiAction) {
    if let Some(op) = action.op_as::<lpa_studio_core::PlaylistActivateOp>() {
        facts.active = Some(op.entry);
        return;
    }
    let Some(op) = action.op_as::<lpa_studio_core::PanelWriteOp>() else {
        return;
    };
    match op.channel.as_str() {
        lpc_model::PLAYLIST_TOUR_CHANNEL => {
            if let Ok(tour) = PlaylistTour::from_lp_value(&op.value) {
                facts.tour = tour;
                *facts = tour_held(facts.clone());
            }
        }
        lpc_model::PLAYLIST_SKIP_CHANNEL => {
            if let Ok(skip) = Vec::<u32>::from_lp_value(&op.value) {
                facts.skip = skip;
                *facts = skip_held(facts.clone());
            }
        }
        _ => {}
    }
}

/// The shared row: what stays put across a switch (vision D15).
fn shared_controls() -> Vec<UiPanelControlView> {
    use crate::app::node::node_story_fixtures::palette_cycle;
    vec![
        at_default(
            fader(PIECE_SCOPE, "brightness", "brightness", 160.0, 255.0),
            "authored 160",
        ),
        at_default(
            knob(PIECE_SCOPE, "speed", "speed", 1.0, 0.25, 4.0, None),
            "authored default",
        ),
        at_default(
            knob(PIECE_SCOPE, "detail", "detail", 1.0, 0.25, 4.0, None),
            "authored default",
        ),
        at_default(
            swatch(PIECE_SCOPE, "palette", "palette", &palette_cycle()),
            "authored palette",
        ),
    ]
}

/// The playing pattern's own knobs, on its MODULE's scope (ruling 2).
fn pattern_knobs(label: &str) -> UiPanelGroup {
    UiPanelGroup::new(label, PLAYING_PATTERN_SCOPE)
        .with_target(scope_target(PLAYING_PATTERN_SCOPE))
        .with_controls(vec![
            at_default(
                knob(
                    PLAYING_PATTERN_SCOPE,
                    "width",
                    "width",
                    2.5,
                    0.5,
                    12.0,
                    None,
                ),
                "authored default",
            ),
            at_default(
                knob(PLAYING_PATTERN_SCOPE, "tail", "tail", 0.45, 0.05, 1.0, None),
                "authored default",
            ),
        ])
}

/// A node name as its card reads (`noise_soft` → `Noise soft`) — Studio's
/// own rule, restated for fixtures.
fn human(raw: &str) -> String {
    let normalized = raw.replace(['_', '-'], " ");
    let mut chars = normalized.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
