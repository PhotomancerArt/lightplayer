//! The drawable picture (plan `drawing-on-the-picture`, P6): the same live
//! picture three ways. Anatomy: `docs/ux/anatomy/drawable-picture.md`.

use dioxus::prelude::*;
use lpa_studio_core::{NodeId, UiDrawSurface, UiDrawTarget, UiDrawable};
use lpa_studio_web_story_macros::story;
use lpc_wire::WireScopeRef;

use crate::app::home::home_gallery_stories::live_card_lamp_frame;
use crate::app::node::lamp_draw::DrawRing;
use crate::app::node::lamp_view::LampView;

#[story(
    description = "A connected board's live picture three ways: NOT DRAWABLE (no pattern reads touches, or a watched card — today's picture, untouched), DRAWABLE at rest (the same picture: a crosshair over it is the only sign, no label, no mode), and MID-STROKE with two fingers (a ring under each, drawn at once while the lamps behind catch up; the one lifting fades)."
)]
fn drawable_picture() -> Element {
    let drawable = UiDrawable {
        surface: UiDrawSurface::Project,
        target: UiDrawTarget {
            scope: WireScopeRef::Module {
                owner: NodeId::new(3),
            },
            channel: "touches".to_string(),
        },
    };
    let rings = vec![
        DrawRing {
            pointer: 1,
            x: 92.0,
            y: 70.0,
            lifted: false,
        },
        DrawRing {
            pointer: 2,
            x: 196.0,
            y: 104.0,
            lifted: false,
        },
    ];
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:gap-4 tw:p-4",
            for (caption, drawable, rings) in [
                ("not drawable", None, Vec::new()),
                ("drawable", Some(drawable.clone()), Vec::new()),
                ("drawing, two fingers", Some(drawable.clone()), rings),
            ] {
                figure { key: "{caption}", class: "tw:m-0 tw:grid tw:gap-1",
                    div { class: "tw:relative tw:h-40 tw:w-72 tw:overflow-hidden tw:rounded-md tw:bg-[#07080a]",
                        div { class: "ux-play-lamps",
                            LampView { preview: live_card_lamp_frame(), drawable, rings }
                        }
                    }
                    figcaption { class: "tw:text-xs tw:text-subtle-foreground", "{caption}" }
                }
            }
        }
    }
}
