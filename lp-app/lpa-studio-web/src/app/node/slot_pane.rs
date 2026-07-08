//! Reusable stand-alone value presentation.
//!
//! A `SlotPane` frames a single value in a bold, self-contained way: the value
//! name sits in a tight title bar alongside the same detail-button affordance
//! every slot surface carries, and the value itself is centered in the body.
//!
//! It is deliberately lighter and tighter than the top-level [`PaneFrame`] node
//! chrome. Produced products and produced values are the first consumers; the
//! binding/bus surfaces will reuse it to show bound values, which is why the
//! [`SlotPaneTreatment`] variants exist even though today's callers only pass
//! `Neutral`.
//!
//! [`PaneFrame`]: crate::app::PaneFrame

use dioxus::prelude::*;
use lpa_studio_core::UiSlotAspect;

use crate::app::node::SlotDetailButton;

/// Visual treatment for a [`SlotPane`], mirroring the slot-affordance language
/// used elsewhere so a bound value on the bus reads the same as a bound slot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SlotPaneTreatment {
    /// Plain value with no status connotation (default for outputs).
    #[default]
    Neutral,
    /// The value is bound through the binding/bus system.
    Bound,
    /// The value carries unsaved authored edits.
    Unsaved,
    /// The value is being written back to the runtime.
    Saving,
    /// The value is present but failed validation.
    Invalid,
    /// The value is in an error state.
    Error,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotPane(
    /// Value name shown in the title bar.
    title: String,
    /// Detail aspects surfaced through the header detail button.
    aspects: Vec<UiSlotAspect>,
    /// Open the detail popup on first render (story/testing affordance).
    #[props(default = false)] initially_open: bool,
    /// Status treatment for the frame chrome.
    #[props(default)] treatment: SlotPaneTreatment,
    /// Shrink the pane to hug its content instead of filling the available
    /// width. Use for values with an intrinsic size (e.g. a capped product
    /// preview) so the pane matches the product rather than framing it in dead
    /// space; leave off for values that should stretch (a produced-value grid).
    #[props(default = false)] fit: bool,
    /// Render the body flush to the pane edges (no padding) so hero media —
    /// e.g. a product preview — bleeds to the frame under the title bar
    /// instead of nesting a second bordered box inside a padded one. The pane's
    /// rounding + `overflow-hidden` clip the media. Leave off for text/number
    /// values that want breathing room.
    #[props(default = false)] flush: bool,
    /// The value display rendered, centered, in the pane body.
    children: Element,
) -> Element {
    let body_class = if flush {
        "tw:grid tw:min-w-0"
    } else {
        "tw:grid tw:min-w-0 tw:place-items-center tw:gap-2 tw:p-3"
    };
    rsx! {
        section { class: slot_pane_frame_class(treatment, fit),
            header { class: slot_pane_header_class(treatment),
                strong { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-bold tw:leading-tight tw:text-strong-foreground",
                    "{title}"
                }
                SlotDetailButton {
                    label: title.clone(),
                    aspects,
                    initially_open,
                }
            }
            div { class: body_class,
                {children}
            }
        }
    }
}

fn slot_pane_frame_class(treatment: SlotPaneTreatment, fit: bool) -> String {
    // `w-fit` sizes the pane to its content's intrinsic width (capped to the
    // container by `max-w-full`); the default lets it stretch to fill.
    let width = if fit {
        "tw:w-fit tw:max-w-full"
    } else {
        "tw:w-full"
    };
    let border = match treatment {
        SlotPaneTreatment::Neutral => "tw:border-border",
        SlotPaneTreatment::Bound => "tw:border-accent-border",
        SlotPaneTreatment::Unsaved => "tw:border-status-warning-border",
        SlotPaneTreatment::Saving => "tw:border-status-working-border",
        SlotPaneTreatment::Invalid | SlotPaneTreatment::Error => "tw:border-status-error-border",
    };
    format!("tw:grid {width} tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border {border} tw:bg-card-subtle")
}

fn slot_pane_header_class(treatment: SlotPaneTreatment) -> &'static str {
    match treatment {
        SlotPaneTreatment::Neutral => {
            "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-card-muted tw:py-1 tw:pl-2.5 tw:pr-1"
        }
        SlotPaneTreatment::Bound => {
            "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg),transparent_72%)] tw:py-1 tw:pl-2.5 tw:pr-1"
        }
        SlotPaneTreatment::Unsaved => {
            "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_72%)] tw:py-1 tw:pl-2.5 tw:pr-1"
        }
        SlotPaneTreatment::Saving => {
            "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg),transparent_72%)] tw:py-1 tw:pl-2.5 tw:pr-1"
        }
        SlotPaneTreatment::Invalid | SlotPaneTreatment::Error => {
            "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg),transparent_72%)] tw:py-1 tw:pl-2.5 tw:pr-1"
        }
    }
}
