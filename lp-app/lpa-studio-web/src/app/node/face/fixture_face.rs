//! The fixture card's permanent face: lit preview + dominant brightness
//! fader, in the flat section grammar.
//!
//! The `output` section is the thing being lit — the control product's LED
//! sample points rendered full-bleed, with mapping view toggles (wiring
//! numbers, arrows, universe colors, live output) layered on the same
//! display. This surface is the fixture's "one home": the read-only mapping
//! view today, the in-place mapping editor later (2D mapping plan M5). The
//! toggle state is view-local for now, same as the drawer open-state (a
//! CardUiState re-home is an existing follow-up).
//!
//! The `controls` section holds one dominant horizontal fader bound to
//! `FixtureDef.brightness.some`.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiFixtureFace as UiFixtureFaceData, UiProductKind};

use crate::app::node::map_view::{MapViewOptions, MapViewToggles};
use crate::app::node::produced_product_view::ProductPreview;
use crate::app::node::{NodeCardSection, PanelControl};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixtureFace(
    face: UiFixtureFaceData,
    /// Open the fader's label-trigger detail popover on first render
    /// (stories).
    #[props(default = false)]
    detail_initially_open: bool,
    /// Initial map view options (stories render deterministic states).
    #[props(default)]
    initial_map_view: Option<MapViewOptions>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    let mut map_view = use_signal(move || initial_map_view.unwrap_or_default());
    let show_toggles = preview.kind == UiProductKind::Control;

    rsx! {
        NodeCardSection { label: "output", first: true,
            div { class: "ux-map-preview-wrap",
                ProductPreview {
                    kind: preview.kind,
                    preview: preview.preview.clone(),
                    tracking: preview.tracking,
                    frame: preview.frame,
                    focus_action: None,
                    on_action,
                    map_view: map_view(),
                }
                if show_toggles {
                    MapViewToggles {
                        value: map_view(),
                        on_change: move |next| map_view.set(next),
                    }
                }
            }
        }
        NodeCardSection { label: "controls",
            div { class: "tw:px-4 tw:py-3",
                PanelControl {
                    control: face.brightness.clone(),
                    detail_initially_open,
                    on_action,
                }
            }
        }
    }
}
