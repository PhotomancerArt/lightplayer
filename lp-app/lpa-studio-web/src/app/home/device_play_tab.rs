//! The device card's ▶ play tab: what this board is running, right now.
//!
//! The honest-device-preview plan's whole point. Its predecessor — the D12
//! hero strip (gallery-rework P05) — re-simulated the project in the
//! browser and hung the result under the card's title bar, where it read as
//! a picture OF THE DEVICE. The 2026-08-05 G2 ruling called that
//! dishonest, and the UX spike
//! (`spikes/device-card-live-fixture/index.html`, section 1) converged the
//! same day on this tab.
//!
//! The rule the whole surface obeys: **every treatment says where the
//! picture came from and how old it is.**
//!
//! | State | frame | pill | meta |
//! |---|---|---|---|
//! | live | device frames | green `live · N fps` | `N fps from device` |
//! | live (sim) | the sim engine's frames | violet `sim · N fps` | `N fps from simulator` |
//! | stale (> 5 s) | the last frame | amber `last frame · N ago` | amber `no frames for N` |
//! | offline | the last frame, dimmed + veiled | neutral `last seen …` | `last seen …` |
//! | no frame | nothing — a sentence | — | the project chip alone |
//!
//! The sim card rides the SAME feed as hardware (G1 ruling 3, overturning
//! the spike's Q5): its ▶ shows the frames the sim engine actually
//! published, never a browser re-simulation. The violet SIM pill stays —
//! not as a liveness state but as identity dress: same truth, different
//! machine.
//!
//! The stale threshold is core's
//! [`FRAME_STALE_AFTER_SECS`](lpa_studio_core::FRAME_STALE_AFTER_SECS) (5 s,
//! the spike gate's number), deliberately generous: a board that hiccups
//! for 700 ms has not stopped working, and a card that flashed amber every
//! time it did would teach the user to ignore amber.

use dioxus::prelude::*;
use lpa_studio_core::core::time_ago::time_ago;
use lpa_studio_core::{
    ControlDisplayLayout, FRAME_STALE_AFTER_SECS, RosterCardState, UiAction, UiDeviceCard,
    UiDeviceProjectChip,
};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::node::lamp_view::LampView;
use crate::base::icon::StudioIconName;
use crate::base::inline_button::InlineButton;
use lpa_studio_core::board_display_name;

/// What the ▶ tab is showing, and therefore how it is dressed.
///
/// There is no Sim variant: the sim card shares every state here (its
/// frames are as real as a board's) and wears its identity as the violet
/// pill instead (`sim` threads through the text/family helpers).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayLiveness {
    /// Frames still arriving (or arrived within the threshold).
    Live,
    /// Frames stopped, past [`FRAME_STALE_AFTER_SECS`]. The picture stays;
    /// only its label changes, because the last frame is still the truth
    /// about what the board was doing.
    Stale,
    /// The link is gone (or not answering) and a frame from this session
    /// survives it — dimmed and veiled: last known, not current.
    Offline,
    /// A project, but no frame to show yet. An empty frame with a sentence
    /// — never a placeholder pattern, which is exactly the lie the hero
    /// strip told.
    Waiting,
}

/// The ▶ tab body.
///
/// `sim` is the card's own sim flag rather than something derived from the
/// card, matching every other body in `device_card.rs`. `open_action` is
/// the SAME editor-attach action the title-bar ⤢ dispatches (G1: the
/// picture carries its own way into the editor) — `None` renders no
/// button, mirroring the ⤢'s disabled rule rather than re-deriving it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PlayTabBody(
    card: UiDeviceCard,
    sim: bool,
    now: f64,
    open_action: Option<UiAction>,
    /// The Reconnect dispatch for a gone device (G1b ruling 8: the
    /// button lives IN the picture box — the box is where the absence
    /// shows). `None` on live/sim cards.
    reconnect_action: Option<UiAction>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Last known, not current — the same pair the card grammar dims for.
    let muted = matches!(
        card.state,
        RosterCardState::Offline { .. } | RosterCardState::NotResponding
    );
    let liveness = play_liveness(&card, muted);

    let mut frame_class = String::from("ux-play-frame");
    if liveness == PlayLiveness::Offline {
        frame_class.push_str(" ux-play-frame-dim");
    }
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
    let pill = play_pill_text(&card, liveness, sim, now);
    let meta = play_meta_text(&card, liveness, sim, now);

    rsx! {
        div { class: "{frame_class}", style: "{frame_style}",
            // The session's own frames. `live` stays true even on an
            // offline card: the neutral-lamp mode is for a layout with no
            // data, whereas here there IS data — just old, which the
            // dimming and the veil say out loud.
            if let Some(frame) = card.frame_preview.clone() {
                if frame.display_layout.is_some() {
                    div { class: "ux-play-lamps",
                        LampView { preview: frame }
                    }
                } else {
                    // Frames without geometry: the device declined the
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
                // The gone device with NO remembered frame shows what we
                // DO remember: the board it is (G1b ruling 10), with the
                // way back right under it. No board on record keeps the
                // honest sentence.
                div { class: "ux-play-empty",
                    if let Some(board_id) = card.board_id.as_deref().filter(|_| muted) {
                        p { class: "ux-play-board", "{board_display_name(board_id)}" }
                    } else {
                        p { class: "tw:m-0", "{waiting_line(muted)}" }
                    }
                    if let Some(action) = reconnect_action.clone() {
                        InlineButton {
                            label: "Reconnect {card.name}",
                            icon: Some(StudioIconName::Usb),
                            text: Some("Reconnect".to_string()),
                            on_press: move |_| on_action.call(action.clone()),
                        }
                    }
                }
            }
            if let Some(pill) = pill {
                span { class: "ux-play-pill {pill_family_class(liveness, sim)}",
                    span { class: "ux-play-dot" }
                    "{pill}"
                }
            }
            if liveness == PlayLiveness::Offline {
                div { class: "ux-play-veil",
                    "offline · last frame"
                    if let Some(action) = reconnect_action.clone() {
                        InlineButton {
                            label: "Reconnect {card.name}",
                            icon: Some(StudioIconName::Usb),
                            text: Some("Reconnect".to_string()),
                            on_press: move |_| on_action.call(action.clone()),
                        }
                    }
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
                    label: "Open {card.name} in the editor",
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
fn project_chip(chip: &UiDeviceProjectChip) -> Element {
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
pub(crate) fn play_frame_aspect(card: &UiDeviceCard) -> Option<f32> {
    let layout = card.frame_preview.as_ref()?.display_layout.as_deref()?;
    let ControlDisplayLayout::Layout2d(layout) = layout;
    let width = layout.width_hint.max(1) as f32;
    let height = layout.height_hint.max(1) as f32;
    Some((width / height).clamp(0.75, 4.0))
}

/// Which state the tab is in.
///
/// Order is load-bearing: "no frame" outranks "offline", because an offline
/// card with nothing to show must not wear a veil captioned "last frame"
/// over an empty box — there is no last frame to veil.
pub(crate) fn play_liveness(card: &UiDeviceCard, muted: bool) -> PlayLiveness {
    if card.frame_preview.is_none() {
        return PlayLiveness::Waiting;
    }
    if muted {
        return PlayLiveness::Offline;
    }
    match card.frame_age_secs {
        Some(age) if age > FRAME_STALE_AFTER_SECS => PlayLiveness::Stale,
        _ => PlayLiveness::Live,
    }
}

/// The pill's tint family. A LIVE sim wears violet — identity, per the
/// app-wide sim-is-violet dress — but a stale or waiting sim keeps the
/// state family: "the frames stopped" outranks "this is the simulator".
fn pill_family_class(liveness: PlayLiveness, sim: bool) -> &'static str {
    match liveness {
        PlayLiveness::Live if sim => "ux-play-pill-sim",
        PlayLiveness::Live => "ux-play-pill-live",
        PlayLiveness::Stale => "ux-play-pill-stale",
        PlayLiveness::Offline | PlayLiveness::Waiting => "ux-play-pill-offline",
    }
}

/// The pill inside the view. `None` renders no pill — a frame with nothing
/// in it has no age to report.
pub(crate) fn play_pill_text(
    card: &UiDeviceCard,
    liveness: PlayLiveness,
    sim: bool,
    now: f64,
) -> Option<String> {
    match liveness {
        PlayLiveness::Waiting => None,
        PlayLiveness::Offline => Some(match card.state {
            RosterCardState::Offline {
                last_seen_at: Some(at),
            } => format!("last seen {}", time_ago(now, at)),
            _ => "not responding".to_string(),
        }),
        PlayLiveness::Stale => Some(format!(
            "last frame · {}",
            frame_age_label(card.frame_age_secs.unwrap_or_default())
        )),
        // fps is the ENGINE's render rate, and real firmware only reports
        // it every 5 s — so a freshly connected board says "live" (or the
        // sim "sim") and nothing more rather than inventing a number.
        PlayLiveness::Live if sim => Some(match card.frame_fps {
            Some(fps) => format!("sim · {} fps", fps.round() as i64),
            None => "sim".to_string(),
        }),
        PlayLiveness::Live => Some(match card.frame_fps {
            Some(fps) => format!("live · {} fps", fps.round() as i64),
            None => "live".to_string(),
        }),
    }
}

/// The meta row's right-hand text: the same fact as the pill, said in the
/// card's own voice rather than the picture's.
pub(crate) fn play_meta_text(
    card: &UiDeviceCard,
    liveness: PlayLiveness,
    sim: bool,
    now: f64,
) -> Option<String> {
    match liveness {
        PlayLiveness::Waiting => None,
        PlayLiveness::Offline => Some(match card.state {
            RosterCardState::Offline {
                last_seen_at: Some(at),
            } => format!("last seen {}", time_ago(now, at)),
            _ => "no frames".to_string(),
        }),
        PlayLiveness::Stale => Some(format!(
            "no frames for {}",
            frame_age_label(card.frame_age_secs.unwrap_or_default())
        )),
        PlayLiveness::Live if sim => Some(match card.frame_fps {
            Some(fps) => format!("{} fps from simulator", fps.round() as i64),
            None => "frames from simulator".to_string(),
        }),
        PlayLiveness::Live => Some(match card.frame_fps {
            Some(fps) => format!("{} fps from device", fps.round() as i64),
            None => "frames from device".to_string(),
        }),
    }
}

/// The empty frame's sentence: why there is no picture, in the terms of
/// whichever reason applies.
///
/// A remembered card is the common case — nothing was ever read off this
/// board in THIS run of the app, and there is no persisted snapshot seam
/// yet, so the honest thing to offer is the way to get one.
fn waiting_line(muted: bool) -> &'static str {
    if muted {
        "No frame from this device yet — reconnect to see what it is running."
    } else {
        "Waiting for the first frame…"
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
    use super::*;

    const NOW: f64 = 1_800_000_000.0;

    fn card(state: RosterCardState) -> UiDeviceCard {
        UiDeviceCard {
            uid: Some("dev7pQr5St89uVwXy2C".to_string()),
            session_key: None,
            name: "DOM-Z-102".to_string(),
            transport: "USB".to_string(),
            state,
            project: Some(UiDeviceProjectChip {
                uid: "prj3fKq8Zr21bTxYw0A".to_string(),
                name: "zook-dome".to_string(),
            }),
            fw: None,
            hardware: None,
            detected_chip: None,
            board_id: None,
            port_label: None,
            safe_clamp: None,
            sim: false,
            console_tail: Vec::new(),
            frame_preview: None,
            frame_age_secs: None,
            frame_fps: None,
            ui: Default::default(),
        }
    }

    /// A card with a frame: the preview's contents are irrelevant to every
    /// rule here, so the cheapest possible one stands in.
    fn with_frame(mut card: UiDeviceCard, age: f64) -> UiDeviceCard {
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

    /// The threshold is a threshold, not a hair trigger: a board that goes
    /// quiet for four seconds is still "live". This is the spike gate's
    /// ruling encoded — short hiccups must stay calm.
    #[test]
    fn frames_stay_live_until_the_stale_threshold() {
        let live = with_frame(card(RosterCardState::RunningUpToDate), 4.0);
        assert_eq!(play_liveness(&live, false), PlayLiveness::Live);
        let stale = with_frame(card(RosterCardState::RunningUpToDate), 6.0);
        assert_eq!(play_liveness(&stale, false), PlayLiveness::Stale);
    }

    /// An offline card with no frame must NOT claim a last frame.
    #[test]
    fn an_offline_card_with_no_frame_waits_instead_of_veiling() {
        let offline = card(RosterCardState::Offline {
            last_seen_at: Some(NOW - 7200.0),
        });
        assert_eq!(play_liveness(&offline, true), PlayLiveness::Waiting);
        assert_eq!(
            play_pill_text(&offline, PlayLiveness::Waiting, false, NOW),
            None
        );

        let with_last_frame = with_frame(offline, 7200.0);
        assert_eq!(play_liveness(&with_last_frame, true), PlayLiveness::Offline);
        assert_eq!(
            play_pill_text(&with_last_frame, PlayLiveness::Offline, false, NOW).as_deref(),
            Some("last seen 2h ago")
        );
    }

    /// The sim rides the same feed and the same states (G1 ruling 3 —
    /// its frames are as real as a board's), wearing violet identity
    /// dress while live and the honest state families otherwise.
    #[test]
    fn the_sim_shares_the_states_and_says_it_is_the_simulator() {
        let mut sim = with_frame(card(RosterCardState::RunningUpToDate), 0.0);
        sim.frame_fps = Some(60.2);
        assert_eq!(play_liveness(&sim, false), PlayLiveness::Live);
        assert_eq!(
            play_pill_text(&sim, PlayLiveness::Live, true, NOW).as_deref(),
            Some("sim · 60 fps")
        );
        assert_eq!(
            play_meta_text(&sim, PlayLiveness::Live, true, NOW).as_deref(),
            Some("60 fps from simulator")
        );
        assert_eq!(
            pill_family_class(PlayLiveness::Live, true),
            "ux-play-pill-sim"
        );
        // Stopped frames outrank identity: a stalled sim goes amber like
        // any other stalled engine.
        assert_eq!(
            pill_family_class(PlayLiveness::Stale, true),
            "ux-play-pill-stale"
        );
    }

    /// fps is the board's own report and arrives up to 5 s late; the pill
    /// must not invent one.
    #[test]
    fn a_rate_appears_only_once_the_board_reports_one() {
        let mut live = with_frame(card(RosterCardState::RunningUpToDate), 0.0);
        assert_eq!(
            play_pill_text(&live, PlayLiveness::Live, false, NOW).as_deref(),
            Some("live")
        );
        live.frame_fps = Some(29.9);
        assert_eq!(
            play_pill_text(&live, PlayLiveness::Live, false, NOW).as_deref(),
            Some("live · 30 fps")
        );
    }

    #[test]
    fn ages_read_in_natural_units() {
        assert_eq!(frame_age_label(6.4), "6 s ago");
        assert_eq!(frame_age_label(89.0), "89 s ago");
        assert_eq!(frame_age_label(240.0), "4 min ago");
        assert_eq!(frame_age_label(7200.0), "2 h ago");
    }
}
