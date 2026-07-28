//! The effect card's permanent face: output-mirror hero → promoted knobs →
//! provenance line.
//!
//! The controls are the effect's curated public API (effects-are-projects
//! ADR): each [`lpa_studio_core::UiPanelControl`] carries the INNER child's
//! slot address, so a knob drag dispatches into the child's artifact through
//! the standard slot write path — dirty dots, bound-violet, and the detail
//! popover are the child row's own. The advanced drawer renders below via
//! [`super::NodeCardDrawers`]; the effect's children render outside as
//! sibling cards (collaborators, all live — no active-child suppression).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiEffectFace as UiEffectFaceData};

use crate::app::node::produced_product_view::ProductPreview;
use crate::app::node::{NodeCardSection, PanelControl};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EffectFace(
    face: UiEffectFaceData,
    /// Open this control's label-trigger detail popover on first render
    /// (stories).
    #[props(default = None)]
    detail_open_control: Option<String>,
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
        if !face.controls.is_empty() {
            NodeCardSection { label: "controls",
                div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:px-4 tw:py-3",
                    for control in face.controls.clone() {
                        PanelControl {
                            key: "{control.label}",
                            detail_initially_open: detail_open_control.as_deref() == Some(control.label.as_str()),
                            control,
                            on_action,
                        }
                    }
                }
            }
        }
        if let Some(provenance) = face.provenance.clone() {
            div { class: "tw:px-4 tw:py-2 tw:text-xs tw:text-[var(--studio-fg-muted)]",
                "{provenance}"
            }
        }
    }
}
