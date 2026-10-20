//! [`BoardPicture`]: the board's lights, filling the card's picture slot.
//!
//! The frame aspect-fit ([`LampView`]), dimmed when core says it is last
//! known rather than current; dark when there is no frame. While an update
//! holds the board's lights, the light strip instead ([`UpdateLightSlot`]:
//! what the board's own LEDs show, one solid colour). While the board is
//! well nothing is drawn over the picture; otherwise the card's surface
//! covers it ([`super::board_surface`]) and the picture takes the cover's
//! one dim (`covered`), never its own stale dim as well. The status corner
//! is cut out of it, not laid on it.

use dioxus::prelude::*;
use lpa_studio_core::{PictureSource, UiBoardPicture, UpdateLight};

use crate::app::node::lamp_view::LampView;

/// The picture. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardPicture(
    picture: UiBoardPicture,
    /// The surface's notice covers the picture: it takes the cover's dim.
    #[props(default)]
    covered: bool,
) -> Element {
    if let Some(light) = picture.light {
        let frame_class = match covered {
            true => COVERED_CLASS,
            false => PICTURE_CLASS,
        };
        return rsx! {
            UpdateLightSlot { light, frame_class }
        };
    }
    let source = source_name(picture.source);
    let frame = picture.frame.filter(|frame| frame.display_layout.is_some());
    let drawn = frame.is_some();
    rsx! {
        div {
            class: picture_class(picture.dim, covered),
            "data-picture": source,
            "data-picture-frame": "{drawn}",
            if let Some(frame) = frame {
                div { class: "ux-play-lamps",
                    LampView { preview: frame }
                }
            }
        }
    }
}

/// The hook's word for where the picture comes from (`data-picture`):
/// `link`, `lens`, `saved` or `none`. A walk reads it — a board another tab
/// holds shows `saved`, the picture that tab left in the library.
fn source_name(source: PictureSource) -> &'static str {
    match source {
        PictureSource::Link => "link",
        PictureSource::Lens => "lens",
        PictureSource::Saved => "saved",
        PictureSource::None => "none",
    }
}

/// The light strip a stopped show leaves: a row of lamps lit solid in the
/// board's light (dark yellow: updating; dark red: waiting for its
/// firmware), so the card and the porch agree — and, where the surface has
/// room for it, the update's sentence under them. Shared with today's
/// device card until it goes.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn UpdateLightSlot(
    light: UpdateLight,
    /// The whole sentence under the lamps; the board card's picture has
    /// none (the firmware details say it).
    #[props(default)]
    sentence: Option<String>,
    /// The frame the strip fills.
    #[props(default = "ux-play-frame ux-play-frame-slot")]
    frame_class: &'static str,
) -> Element {
    rsx! {
        div { class: "{frame_class} ux-update-light {update_light_class(light)}",
            div { class: "ux-update-leds", aria_hidden: "true",
                for lamp in 0..UPDATE_LIGHT_LAMPS {
                    span { key: "{lamp}", class: "ux-update-led" }
                }
            }
            if let Some(sentence) = sentence {
                p { class: "ux-update-light-sentence", title: "{sentence}", "{sentence}" }
            }
        }
    }
}

/// How many lamps the light strip draws: one row across the slot.
pub(crate) const UPDATE_LIGHT_LAMPS: usize = 16;

/// The light's colour family (style.css `--studio-update-light-*`).
fn update_light_class(light: UpdateLight) -> &'static str {
    match light {
        UpdateLight::DarkYellow => "ux-update-light-yellow",
        UpdateLight::DarkRed => "ux-update-light-red",
    }
}

/// The picture fills its slot (the card's fixed picture row), dark under
/// the lamps.
const PICTURE_CLASS: &str = "ux-board-picture tw:absolute tw:inset-0 tw:bg-[#07080a]";

/// [`PICTURE_CLASS`] under the surface's notice, the frame or the update's
/// light strip: the cover's dim (style.css `.ux-board-picture-covered`).
const COVERED_CLASS: &str =
    "ux-board-picture tw:absolute tw:inset-0 tw:bg-[#07080a] ux-board-picture-covered";

/// [`PICTURE_CLASS`], dimmed when the picture is last known, not current;
/// under the surface's notice, the cover's one dim instead.
fn picture_class(dim: bool, covered: bool) -> &'static str {
    match (covered, dim) {
        (true, _) => COVERED_CLASS,
        (false, true) => {
            "ux-board-picture tw:absolute tw:inset-0 tw:bg-[#07080a] ux-play-frame-dim"
        }
        (false, false) => PICTURE_CLASS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picture fills its slot and never sets a height of its own: the
    /// card's picture row is the one height rule.
    #[test]
    fn the_picture_fills_its_slot_and_dims_only_when_asked() {
        for dim in [false, true] {
            for covered in [false, true] {
                let class = picture_class(dim, covered);
                assert!(class.contains("tw:absolute tw:inset-0"), "{class}");
                assert!(!class.contains("tw:h-"), "{class}");
            }
        }
        assert!(picture_class(true, false).contains("ux-play-frame-dim"));
        assert!(!picture_class(false, false).contains("dim"));
        // One dim: covered, a stale picture takes the cover's, not both.
        let covered = picture_class(true, true);
        assert!(covered.contains("ux-board-picture-covered"), "{covered}");
        assert!(!covered.contains("ux-play-frame-dim"), "{covered}");
        assert_eq!(
            update_light_class(UpdateLight::DarkRed),
            "ux-update-light-red"
        );
    }
}
