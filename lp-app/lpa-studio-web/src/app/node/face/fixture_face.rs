//! The fixture card's permanent face: lit preview + dominant brightness
//! fader, in the flat section grammar.
//!
//! The `output` section is the thing being lit — the control product's LED
//! sample points (the existing control-preview treatment), rendered
//! full-bleed. The `controls` section holds one dominant horizontal fader
//! bound to `FixtureDef.brightness.some`. The mapping editor is a later
//! custom drawer, deliberately absent.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiFixtureFace as UiFixtureFaceData};

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
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();

    rsx! {
        NodeCardSection { label: "output", first: true,
            ProductPreview {
                kind: preview.kind,
                preview: preview.preview.clone(),
                tracking: preview.tracking,
                frame: preview.frame,
                focus_action: None,
                on_action,
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
