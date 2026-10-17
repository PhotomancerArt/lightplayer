//! [`BoardSurface`]: the top of the board card says what matters
//! (`docs/ux/anatomy/board-card.md`, "Surface"; core's
//! [`lpa_studio_core::UiBoardSurface`]). Concept B of the 2026-10-17 spike,
//! "the notice as an object": the state's icon in a circle, in the state's
//! tone; then one or two plain sentences, centred — the tone is the icon's
//! alone, the sentences stay plain; under them the work's progress, a bar
//! and its number, or the spinner in the icon's place. A picture dims under
//! it, fully covered (the cover's one dim: [`super::board_picture`] drops
//! its own). There are no buttons on the surface: the name bar's primary is
//! the thing to do.
//!
//! While the board is well there is nothing to draw: the picture is the
//! surface. `data-surface` names the state (`old-firmware`, `offline`, …),
//! the walks' hook for what the card says.

use dioxus::prelude::*;
use lpa_studio_core::{
    SurfacePicture, SurfaceProgress, UiBoardSurface, UiStatusKind, UiSurfaceNotice,
};

use crate::base::{StudioIcon, action_icon_name, conic_spinner_class, iridescent_fill_class};

/// The surface. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardSurface(surface: UiBoardSurface) -> Element {
    let state = surface.state.as_str();
    let Some(notice) = surface.notice else {
        return rsx! {
            div { class: "tw:hidden", "data-surface": state }
        };
    };
    let spinner = surface.progress == Some(SurfaceProgress::Spinner);
    let percent = match surface.progress {
        Some(SurfaceProgress::Percent(percent)) => Some(percent),
        Some(SurfaceProgress::Spinner) | None => None,
    };
    rsx! {
        div {
            class: SURFACE_CLASS,
            "data-surface": state,
            role: "status",
            if surface.picture == SurfacePicture::Dimmed {
                div { class: SCRIM_CLASS, aria_hidden: "true" }
            }
            div { class: INNER_CLASS,
                NoticeIcon { notice: notice.clone(), spinner }
                p { class: FIRST_CLASS, "data-surface-first": "true", "{notice.first}" }
                if let Some(second) = notice.second.as_ref() {
                    p { class: SECOND_CLASS, "data-surface-second": "true", "{second}" }
                }
                if let Some(percent) = percent {
                    div { class: PROGRESS_CLASS, "data-surface-progress": "{percent}",
                        span { class: TRACK_CLASS, aria_hidden: "true",
                            span {
                                class: "{iridescent_fill_class()} {FILL_CLASS}",
                                style: "width: {percent}%;",
                            }
                        }
                        span { class: PERCENT_CLASS, "{percent}%" }
                    }
                }
            }
        }
    }
}

/// The state's icon in its circle, in the state's tone; the spinner in its
/// place while the work has no number.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NoticeIcon(notice: UiSurfaceNotice, spinner: bool) -> Element {
    let icon = action_icon_name(Some(notice.icon.as_str()));
    rsx! {
        span {
            class: ICON_CLASS,
            "data-tone": tone_name(notice.tone),
            aria_hidden: "true",
            if spinner {
                span { class: "{conic_spinner_class()} ux-board-spinner" }
            } else if let Some(icon) = icon {
                StudioIcon { name: icon, size: 15 }
            }
        }
    }
}

/// The tone's hook word, which style.css tints the icon's circle by
/// (`.ux-board-surface-icon[data-tone=…]`): only the families a notice
/// takes — attention, error, working — tint it; the rest are plain.
pub(crate) fn tone_name(tone: UiStatusKind) -> &'static str {
    match tone {
        UiStatusKind::Attention => "attention",
        UiStatusKind::Error => "error",
        UiStatusKind::Working => "working",
        UiStatusKind::Neutral | UiStatusKind::Good | UiStatusKind::Live | UiStatusKind::Warning => {
            "neutral"
        }
    }
}

/// The surface fills the picture's slot, over the picture and under the
/// corner (style.css: `.ux-board-surface`, with the narrow card's sizes).
const SURFACE_CLASS: &str = "ux-board-surface";
/// The cover over a dimmed picture.
const SCRIM_CLASS: &str = "ux-board-surface-scrim";
/// The icon, the sentences and the progress, centred.
const INNER_CLASS: &str = "ux-board-surface-inner";
/// The icon's circle.
const ICON_CLASS: &str = "ux-board-surface-icon";
/// What is going on: plain, strong.
const FIRST_CLASS: &str = "ux-board-surface-first";
/// The button said aloud: plain, quieter.
const SECOND_CLASS: &str = "ux-board-surface-second";
/// The work's bar and its number.
const PROGRESS_CLASS: &str = "ux-board-surface-progress";
const TRACK_CLASS: &str = "ux-board-surface-track";
const FILL_CLASS: &str = "ux-board-surface-fill";
const PERCENT_CLASS: &str = "ux-board-surface-percent";

#[cfg(test)]
mod tests {
    use lpa_studio_core::SurfaceState;

    use super::super::card_test_fixtures::render;
    use super::*;

    /// A notice draws its icon, its sentences and its progress, and never
    /// a button: the primary is the thing to do.
    #[test]
    fn a_notice_is_an_icon_and_plain_sentences_and_no_button() {
        let html = render(
            surface,
            UiBoardSurface {
                state: SurfaceState::Updating,
                picture: SurfacePicture::Dimmed,
                notice: Some(UiSurfaceNotice {
                    icon: "firmware".to_string(),
                    tone: UiStatusKind::Working,
                    first: "This board is updating its firmware.".to_string(),
                    second: None,
                }),
                progress: Some(SurfaceProgress::Percent(42)),
            },
        );
        assert!(html.contains("data-surface=\"updating\""), "{html}");
        assert!(html.contains(SCRIM_CLASS), "a dimmed picture is covered");
        assert!(
            html.contains(">This board is updating its firmware.<"),
            "{html}"
        );
        assert!(html.contains("data-surface-progress=\"42\""), "{html}");
        assert!(html.contains("width: 42%;"), "{html}");
        assert!(html.contains(">42%<"), "{html}");
        assert!(
            !html.contains("<button"),
            "no buttons on the surface: {html}"
        );
        assert!(html.contains("data-tone=\"working\""), "{html}");
    }

    /// The tone is the icon's alone: the sentences carry no tone class.
    #[test]
    fn the_tone_is_the_icons_alone() {
        let html = render(
            surface,
            UiBoardSurface {
                state: SurfaceState::OldFirmware,
                picture: SurfacePicture::None,
                notice: Some(UiSurfaceNotice {
                    icon: "firmware".to_string(),
                    tone: UiStatusKind::Attention,
                    first: "This board is running old firmware.".to_string(),
                    second: Some("Update the firmware to connect and edit.".to_string()),
                }),
                progress: None,
            },
        );
        assert!(html.contains("data-tone=\"attention\""), "{html}");
        assert!(!html.contains(SCRIM_CLASS), "nothing to cover");
        assert!(
            html.contains(">Update the firmware to connect and edit.<"),
            "{html}"
        );
        for class in [FIRST_CLASS, SECOND_CLASS] {
            assert!(!class.contains("status"), "{class}");
        }
        // The one toned element is the icon's circle.
        assert_eq!(html.matches("data-tone=").count(), 1, "{html}");
    }

    /// While the board is well the picture is the surface: nothing drawn,
    /// the state still named for the walks.
    #[test]
    fn a_board_that_is_well_draws_nothing_over_its_picture() {
        let html = render(
            surface,
            UiBoardSurface {
                state: SurfaceState::AllWell,
                picture: SurfacePicture::Live,
                notice: None,
                progress: None,
            },
        );
        assert!(html.contains("data-surface=\"all-well\""), "{html}");
        assert!(!html.contains(INNER_CLASS), "{html}");
    }

    /// Work with no number spins in the icon's place.
    #[test]
    fn work_with_no_number_spins_in_the_icons_place() {
        let html = render(
            surface,
            UiBoardSurface {
                state: SurfaceState::Connecting,
                picture: SurfacePicture::None,
                notice: Some(UiSurfaceNotice {
                    icon: "usb".to_string(),
                    tone: UiStatusKind::Working,
                    first: "Connecting to this board over USB.".to_string(),
                    second: None,
                }),
                progress: Some(SurfaceProgress::Spinner),
            },
        );
        assert!(html.contains(conic_spinner_class()), "{html}");
        assert!(!html.contains("data-surface-progress"), "{html}");
    }

    fn surface(surface: UiBoardSurface) -> Element {
        rsx! {
            BoardSurface { surface }
        }
    }
}
