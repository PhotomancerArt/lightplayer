//! The sim card's ▶ play tab: what the simulator is running, right now.
//!
//! The honest-device-preview plan's whole point. Its predecessor — the D12
//! hero strip (gallery-rework P05) — re-simulated the project in the
//! browser and hung the result under the card's title bar, where it read as
//! a picture OF THE RUNTIME. The 2026-08-05 G2 ruling called that
//! dishonest, and the UX spike
//! (`spikes/device-card-live-fixture/index.html`, section 1) converged the
//! same day on this tab.
//!
//! The rule the whole surface obeys: **every treatment says where the
//! picture came from and how old it is.**
//!
//! | State | frame | pill | meta |
//! |---|---|---|---|
//! | live | the sim engine's frames | violet `sim · N fps` | `N fps from simulator` |
//! | stale (> 5 s) | the last frame | amber `last frame · N ago` | amber `no frames for N` |
//! | no frame | nothing — a sentence | — | the project chip alone |
//!
//! The sim rides the SAME feed hardware will (G1 ruling 3, overturning the
//! spike's Q5): its ▶ shows the frames the sim engine actually published,
//! never a browser re-simulation. The violet SIM pill stays — not as a
//! liveness state but as identity dress: same truth, different machine.
//!
//! ⚠️ The offline/not-responding treatments (the dim + veil + Reconnect)
//! went with M2 of the device-model rebuild: a sim session's card exists
//! only while the session does, so it is never "remembered". The rebuilt
//! device model re-adds them.
//!
//! The stale threshold is core's
//! [`FRAME_STALE_AFTER_SECS`](lpa_studio_core::FRAME_STALE_AFTER_SECS) (5 s,
//! the spike gate's number), deliberately generous: a runtime that hiccups
//! for 700 ms has not stopped working, and a card that flashed amber every
//! time it did would teach the user to ignore amber.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControlDisplayLayout, FRAME_STALE_AFTER_SECS, UiAction, UiSimCard, UiSimProjectChip,
};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::node::lamp_view::LampView;
use crate::base::icon::StudioIconName;
use crate::base::inline_button::InlineButton;

/// What the ▶ tab is showing, and therefore how it is dressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayLiveness {
    /// Frames still arriving (or arrived within the threshold).
    Live,
    /// Frames stopped, past [`FRAME_STALE_AFTER_SECS`]. The picture stays;
    /// only its label changes, because the last frame is still the truth
    /// about what the runtime was doing.
    Stale,
    /// A project, but no frame to show yet. An empty frame with a sentence
    /// — never a placeholder pattern, which is exactly the lie the hero
    /// strip told.
    Waiting,
}

/// The ▶ tab body.
///
/// `open_action` is the SAME editor-attach action the title-bar ⤢
/// dispatches (G1: the picture carries its own way into the editor) —
/// `None` renders no button, mirroring the ⤢'s disabled rule rather than
/// re-deriving it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PlayTabBody(
    card: UiSimCard,
    open_action: Option<UiAction>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let liveness = play_liveness(&card);

    let mut frame_class = String::from("ux-play-frame");
    // The frame wears the LAYOUT's own aspect ratio, so a strip is wide,
    // a dome is square, and nothing is ever squished to fit a fixed box —
    // the card grows instead (G1b feedback, 2026-08-05). The ratio is
    // clamped core-side; the CSS side ALSO caps the absolute size (a
    // matched max-height + max-width pair, so the cap never re-squishes)
    // — in a wide container (pane mode, a stretched gallery column) an
    // uncapped 100%-width square hit viewport scale (G1b follow-up).
    // No layout keeps the CSS default height.
    let frame_style = play_frame_aspect(&card)
        .map(|aspect| format!("--play-aspect: {aspect:.4};"))
        .unwrap_or_default();
    if !frame_style.is_empty() {
        frame_class.push_str(" ux-play-frame-fit");
    }
    let pill = play_pill_text(&card, liveness);
    let meta = play_meta_text(&card, liveness);

    rsx! {
        div { class: "{frame_class}", style: "{frame_style}",
            if let Some(frame) = card.frame_preview.clone() {
                if frame.display_layout.is_some() {
                    div { class: "ux-play-lamps",
                        LampView { preview: frame }
                    }
                } else {
                    // Frames without geometry: the runtime declined the
                    // display layout (genuinely over the link's budget at
                    // this scale). Say so instead of painting nothing —
                    // a wordless blank canvas reads as a defect.
                    div { class: "ux-play-empty",
                        p { class: "tw:m-0",
                            "Frames are flowing, but this project's lamp layout is too large to preview over this link."
                        }
                    }
                }
            }
            if liveness == PlayLiveness::Waiting {
                div { class: "ux-play-empty",
                    p { class: "tw:m-0", "Waiting for the first frame…" }
                }
            }
            if let Some(pill) = pill {
                span { class: "ux-play-pill {pill_family_class(liveness)}",
                    span { class: "ux-play-dot" }
                    "{pill}"
                }
            }
        }
        div { class: "ux-play-meta",
            if let Some(chip) = card.project.as_ref() {
                {project_chip(chip)}
            }
            span { class: "ux-play-spring" }
            if let Some(meta) = meta {
                span {
                    class: if liveness == PlayLiveness::Stale { "ux-play-fps ux-play-fps-stale" } else { "ux-play-fps" },
                    "{meta}"
                }
            }
            if let Some(action) = open_action {
                InlineButton {
                    label: "Open the simulator's project in the editor",
                    icon: Some(StudioIconName::Grow),
                    text: Some("Editor".to_string()),
                    on_press: move |_| on_action.call(action.clone()),
                }
            }
        }
    }
}

/// The project's identity on the tab: the same seeded swatch the gallery
/// thumbs use, plus the truncating name. Identity, never health — the pill
/// and the meta text carry every claim about liveness.
fn project_chip(chip: &UiSimProjectChip) -> Element {
    let style = thumb_swatch_style(&chip.uid, false);
    rsx! {
        span { class: "ux-play-project",
            span { class: "ux-play-swatch", style: "{style}" }
            span { class: "ux-play-project-name", title: "{chip.name}", "{chip.name}" }
        }
    }
}

/// The frame's aspect ratio (width / height) from the current frame's
/// display layout hints, when there is one. Clamped to [0.75, 4.0]: the
/// picture stays honest in PROPORTION while the card's growth stays
/// bounded — beyond the clamp the lamp field letterboxes inside its own
/// normalized square rather than distorting.
pub(crate) fn play_frame_aspect(card: &UiSimCard) -> Option<f32> {
    let layout = card.frame_preview.as_ref()?.display_layout.as_deref()?;
    let ControlDisplayLayout::Layout2d(layout) = layout;
    let width = layout.width_hint.max(1) as f32;
    let height = layout.height_hint.max(1) as f32;
    Some((width / height).clamp(0.75, 4.0))
}

/// Which state the tab is in.
pub(crate) fn play_liveness(card: &UiSimCard) -> PlayLiveness {
    if card.frame_preview.is_none() {
        return PlayLiveness::Waiting;
    }
    match card.frame_age_secs {
        Some(age) if age > FRAME_STALE_AFTER_SECS => PlayLiveness::Stale,
        _ => PlayLiveness::Live,
    }
}

/// The pill's tint family. A LIVE sim wears violet — identity, per the
/// app-wide sim-is-violet dress — but a stale or waiting sim keeps the
/// state family: "the frames stopped" outranks "this is the simulator".
fn pill_family_class(liveness: PlayLiveness) -> &'static str {
    match liveness {
        PlayLiveness::Live => "ux-play-pill-sim",
        PlayLiveness::Stale => "ux-play-pill-stale",
        PlayLiveness::Waiting => "ux-play-pill-offline",
    }
}

/// The pill inside the view. `None` renders no pill — a frame with nothing
/// in it has no age to report.
pub(crate) fn play_pill_text(card: &UiSimCard, liveness: PlayLiveness) -> Option<String> {
    match liveness {
        PlayLiveness::Waiting => None,
        PlayLiveness::Stale => Some(format!(
            "last frame · {}",
            frame_age_label(card.frame_age_secs.unwrap_or_default())
        )),
        // fps is the ENGINE's render rate, so a runtime that has not
        // reported one yet says "sim" and nothing more rather than
        // inventing a number.
        PlayLiveness::Live => Some(match card.frame_fps {
            Some(fps) => format!("sim · {} fps", fps.round() as i64),
            None => "sim".to_string(),
        }),
    }
}

/// The meta row's right-hand text: the same fact as the pill, said in the
/// card's own voice rather than the picture's.
pub(crate) fn play_meta_text(card: &UiSimCard, liveness: PlayLiveness) -> Option<String> {
    match liveness {
        PlayLiveness::Waiting => None,
        PlayLiveness::Stale => Some(format!(
            "no frames for {}",
            frame_age_label(card.frame_age_secs.unwrap_or_default())
        )),
        PlayLiveness::Live => Some(match card.frame_fps {
            Some(fps) => format!("{} fps from simulator", fps.round() as i64),
            None => "frames from simulator".to_string(),
        }),
    }
}

/// A frame age in units that read naturally (the unit-awareness principle):
/// seconds while seconds are meaningful, then minutes, then hours. A stale
/// pill counting "412 s" is arithmetic homework.
pub(crate) fn frame_age_label(age_secs: f64) -> String {
    let age = age_secs.max(0.0);
    if age < 90.0 {
        format!("{} s ago", age.round() as i64)
    } else if age < 5400.0 {
        format!("{} min ago", (age / 60.0).round() as i64)
    } else {
        format!("{} h ago", (age / 3600.0).round() as i64)
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::SimCardState;

    use super::*;

    fn card() -> UiSimCard {
        UiSimCard {
            state: SimCardState::Running,
            project: Some(UiSimProjectChip {
                uid: "prj3fKq8Zr21bTxYw0A".to_string(),
                name: "zook-dome".to_string(),
            }),
            board_id: None,
            console_tail: Vec::new(),
            frame_preview: None,
            frame_age_secs: None,
            frame_fps: None,
            ui: Default::default(),
        }
    }

    /// A card with a frame: the preview's contents are irrelevant to every
    /// rule here, so the cheapest possible one stands in.
    fn with_frame(mut card: UiSimCard, age: f64) -> UiSimCard {
        card.frame_preview = Some(lpa_studio_core::UiControlProductPreview {
            revision: 1,
            extent: lpa_studio_core::ControlExtent::new(1, 3),
            sample_format: lpa_studio_core::UiControlSampleFormat::U16,
            sample_layout: lpa_studio_core::ControlSampleLayout { spans: Vec::new() },
            display_layout: None,
            bytes: Vec::new().into(),
        });
        card.frame_age_secs = Some(age);
        card
    }

    /// The threshold is a threshold, not a hair trigger: a runtime that
    /// goes quiet for four seconds is still "live". This is the spike
    /// gate's ruling encoded — short hiccups must stay calm.
    #[test]
    fn frames_stay_live_until_the_stale_threshold() {
        let live = with_frame(card(), 4.0);
        assert_eq!(play_liveness(&live), PlayLiveness::Live);
        let stale = with_frame(card(), 6.0);
        assert_eq!(play_liveness(&stale), PlayLiveness::Stale);
    }

    /// No frame, no claims: the pill and the meta row both stay silent
    /// rather than reporting an age nothing has.
    #[test]
    fn a_card_with_no_frame_makes_no_liveness_claim() {
        let waiting = card();
        assert_eq!(play_liveness(&waiting), PlayLiveness::Waiting);
        assert_eq!(play_pill_text(&waiting, PlayLiveness::Waiting), None);
        assert_eq!(play_meta_text(&waiting, PlayLiveness::Waiting), None);
    }

    /// A live sim without a reported rate says what it is, not a number it
    /// does not have.
    #[test]
    fn a_live_sim_without_an_fps_report_says_only_sim() {
        let live = with_frame(card(), 0.5);
        assert_eq!(
            play_pill_text(&live, PlayLiveness::Live).as_deref(),
            Some("sim")
        );
        let mut with_fps = live.clone();
        with_fps.frame_fps = Some(59.6);
        assert_eq!(
            play_pill_text(&with_fps, PlayLiveness::Live).as_deref(),
            Some("sim · 60 fps")
        );
        assert_eq!(
            play_meta_text(&with_fps, PlayLiveness::Live).as_deref(),
            Some("60 fps from simulator")
        );
    }

    /// The stale label reads in natural units (the unit-awareness
    /// principle) — never "412 s".
    #[test]
    fn frame_ages_read_in_natural_units() {
        assert_eq!(frame_age_label(6.0), "6 s ago");
        assert_eq!(frame_age_label(412.0), "7 min ago");
        assert_eq!(frame_age_label(7_200.0), "2 h ago");
    }
}
