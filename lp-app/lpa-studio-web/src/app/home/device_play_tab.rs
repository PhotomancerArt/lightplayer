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
//! | stale (> 5 s) | the last device frame | amber `last frame · N ago` | amber `no frames for N` |
//! | offline | the last device frame, dimmed + veiled | neutral `last seen …` | `last seen …` |
//! | sim | the sim's OWN re-simulation | violet `sim` | `browser simulation` |
//! | no frame | nothing — a sentence | — | the project chip alone |
//!
//! Re-simulation is honest on the sim card and nowhere else: the sim card
//! IS the simulator, so simulating is the thing it does. That is why the
//! violet SIM pill exists rather than a green one.
//!
//! The stale threshold is core's
//! [`FRAME_STALE_AFTER_SECS`](lpa_studio_core::FRAME_STALE_AFTER_SECS) (5 s,
//! the spike gate's number), deliberately generous: a board that hiccups
//! for 700 ms has not stopped working, and a card that flashed amber every
//! time it did would teach the user to ignore amber.

use dioxus::prelude::*;
use lpa_studio_core::core::time_ago::time_ago;
use lpa_studio_core::{
    FRAME_STALE_AFTER_SECS, PreviewSource, RosterCardState, UiDeviceCard, UiDeviceProjectChip,
};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::gallery_preview::use_thumb_preview;
use crate::app::node::lamp_view::LampView;

/// What the ▶ tab is showing, and therefore how it is dressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayLiveness {
    /// Device frames still arriving (or arrived within the threshold).
    Live,
    /// Frames stopped, past [`FRAME_STALE_AFTER_SECS`]. The picture stays;
    /// only its label changes, because the last frame is still the truth
    /// about what the board was doing.
    Stale,
    /// The link is gone (or not answering) and a frame from this session
    /// survives it — dimmed and veiled: last known, not current.
    Offline,
    /// The sim's own re-simulation, marked as such.
    Sim,
    /// A project, but no frame to show yet. An empty frame with a sentence
    /// — never a placeholder pattern, which is exactly the lie the hero
    /// strip told.
    Waiting,
}

/// The ▶ tab body.
///
/// `sim` is the card's own sim flag rather than something derived from the
/// card, matching every other body in `device_card.rs`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PlayTabBody(card: UiDeviceCard, sim: bool, now: f64) -> Element {
    // Last known, not current — the same pair the card grammar dims for.
    let muted = matches!(
        card.state,
        RosterCardState::Offline { .. } | RosterCardState::NotResponding
    );
    let liveness = play_liveness(&card, sim, muted);

    // A LEASE, not a device read: only the sim card ever gets one. The hook
    // runs unconditionally (Dioxus hook order) and is fully inert with a
    // `None` source — no host, no canvas, no observer.
    let sim_source = (liveness == PlayLiveness::Sim)
        .then(|| card.project.as_ref().map(|chip| chip.uid.clone()))
        .flatten()
        .map(PreviewSource::ProjectUid);
    let preview = use_thumb_preview(sim_source);

    let frame_class = if liveness == PlayLiveness::Offline {
        "ux-play-frame ux-play-frame-dim"
    } else {
        "ux-play-frame"
    };
    let pill = play_pill_text(&card, liveness, now);
    let meta = play_meta_text(&card, liveness, now);

    rsx! {
        div { id: "{preview.frame_id}", class: "{frame_class}",
            // The device's own frames. `live` stays true even on an offline
            // card: the neutral-lamp mode is for a layout with no data,
            // whereas here there IS data — just old, which the dimming and
            // the veil say out loud.
            if let Some(frame) = card.frame_preview.clone() {
                div { class: "ux-play-lamps",
                    LampView { preview: frame }
                }
            }
            // The sim's re-simulated canvas, revealed on its first frame.
            if let Some(canvas) = preview.canvas {
                canvas {
                    key: "{canvas.id}",
                    id: "{canvas.id}",
                    width: "320",
                    height: "180",
                    class: if canvas.revealed { "ux-play-canvas is-revealed" } else { "ux-play-canvas" },
                }
            }
            if liveness == PlayLiveness::Waiting {
                p { class: "ux-play-empty", "{waiting_line(muted)}" }
            }
            if let Some(pill) = pill {
                span { class: "ux-play-pill {pill_family_class(liveness)}",
                    span { class: "ux-play-dot" }
                    "{pill}"
                }
            }
            if liveness == PlayLiveness::Offline {
                div { class: "ux-play-veil", "offline · last frame" }
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

/// Which state the tab is in.
///
/// Order is load-bearing: "no frame" outranks "offline", because an offline
/// card with nothing to show must not wear a veil captioned "last frame"
/// over an empty box — there is no last frame to veil.
pub(crate) fn play_liveness(card: &UiDeviceCard, sim: bool, muted: bool) -> PlayLiveness {
    if sim {
        return PlayLiveness::Sim;
    }
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

fn pill_family_class(liveness: PlayLiveness) -> &'static str {
    match liveness {
        PlayLiveness::Live => "ux-play-pill-live",
        PlayLiveness::Stale => "ux-play-pill-stale",
        PlayLiveness::Offline | PlayLiveness::Waiting => "ux-play-pill-offline",
        PlayLiveness::Sim => "ux-play-pill-sim",
    }
}

/// The pill inside the view. `None` renders no pill — a frame with nothing
/// in it has no age to report.
pub(crate) fn play_pill_text(
    card: &UiDeviceCard,
    liveness: PlayLiveness,
    now: f64,
) -> Option<String> {
    match liveness {
        PlayLiveness::Sim => Some("sim".to_string()),
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
        // fps is the BOARD's render rate, and real firmware only reports it
        // every 5 s — so a freshly connected board says "live" and nothing
        // more rather than inventing a number.
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
    now: f64,
) -> Option<String> {
    match liveness {
        PlayLiveness::Sim => Some("browser simulation".to_string()),
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
            uid: Some("dev_7pQr5St89uVwXy2C".to_string()),
            session_key: None,
            name: "DOM-Z-102".to_string(),
            transport: "USB".to_string(),
            state,
            project: Some(UiDeviceProjectChip {
                uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
                name: "zook-dome".to_string(),
            }),
            fw: None,
            hardware: None,
            detected_chip: None,
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
        assert_eq!(play_liveness(&live, false, false), PlayLiveness::Live);
        let stale = with_frame(card(RosterCardState::RunningUpToDate), 6.0);
        assert_eq!(play_liveness(&stale, false, false), PlayLiveness::Stale);
    }

    /// An offline card with no frame must NOT claim a last frame.
    #[test]
    fn an_offline_card_with_no_frame_waits_instead_of_veiling() {
        let offline = card(RosterCardState::Offline {
            last_seen_at: Some(NOW - 7200.0),
        });
        assert_eq!(play_liveness(&offline, false, true), PlayLiveness::Waiting);
        assert_eq!(play_pill_text(&offline, PlayLiveness::Waiting, NOW), None);

        let with_last_frame = with_frame(offline, 7200.0);
        assert_eq!(
            play_liveness(&with_last_frame, false, true),
            PlayLiveness::Offline
        );
        assert_eq!(
            play_pill_text(&with_last_frame, PlayLiveness::Offline, NOW).as_deref(),
            Some("last seen 2h ago")
        );
    }

    /// The sim never wears a device's clothes, frame or no frame.
    #[test]
    fn the_sim_says_it_is_a_simulation() {
        let sim = card(RosterCardState::RunningUpToDate);
        assert_eq!(play_liveness(&sim, true, false), PlayLiveness::Sim);
        assert_eq!(
            play_meta_text(&sim, PlayLiveness::Sim, NOW).as_deref(),
            Some("browser simulation")
        );
    }

    /// fps is the board's own report and arrives up to 5 s late; the pill
    /// must not invent one.
    #[test]
    fn a_rate_appears_only_once_the_board_reports_one() {
        let mut live = with_frame(card(RosterCardState::RunningUpToDate), 0.0);
        assert_eq!(
            play_pill_text(&live, PlayLiveness::Live, NOW).as_deref(),
            Some("live")
        );
        live.frame_fps = Some(29.9);
        assert_eq!(
            play_pill_text(&live, PlayLiveness::Live, NOW).as_deref(),
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
