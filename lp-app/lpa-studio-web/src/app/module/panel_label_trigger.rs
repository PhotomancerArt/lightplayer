//! [`PanelLabelTrigger`]: a panel control's label as its details trigger,
//! with the let-go glyph hung off its end while the control is held — how
//! the editor's and the play page's panel ([`super::ModulePanelControl`])
//! reach a control's details and let go of it.
//!
//! The **label is the detail trigger**, reusing the node face's
//! [`SlotDetailButton`] machinery verbatim: the whole control is the
//! popover's outline anchor, so opening merges the control's outline into
//! the aspect card and reads as diving into it, and a live top-layer copy of
//! the control (`anchor_visual`) paints over the anchor while the popup is
//! open.
//!
//! **Reset** (P2 clear) stays on the face: the small revert glyph sits
//! beside the label ONLY while engaged. It is a gesture, not a detail, so it
//! must not cost a popup to reach. It sends
//! [`PanelGesture::ClearControl`] through the panel's own gesture seam.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPanelTarget, UiSlotAspect};

use crate::app::node::SlotDetailButton;
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName};

use super::PanelGesture;

/// Bare text button for the label trigger; the state color rides the inner
/// visual and the hover affordance is a dotted underline.
const LABEL_TRIGGER_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:leading-none tw:decoration-dotted tw:underline-offset-2 tw:hover:underline";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelLabelTrigger(
    /// The control's label.
    label: String,
    /// The details popup's sections ([`lpa_studio_core::UiPanelControlView::detail_aspects`]).
    aspects: Vec<UiSlotAspect>,
    /// The label's colour family for the control's state.
    color_class: &'static str,
    /// The id of the element the details' outline welds to: the whole
    /// control.
    anchor_id: String,
    /// The control's live copy, painted over the anchor while open.
    anchor_visual: Element,
    /// Open the details on first render (stories).
    #[props(default = false)]
    initially_open: bool,
    /// Whether a panel write holds the control: only then is there
    /// something to let go of.
    #[props(default = false)]
    engaged: bool,
    /// What the let-go clears. A fixture control without one renders no
    /// glyph, even when staged as engaged.
    #[props(default)]
    target: Option<UiPanelTarget>,
    /// The panel's gesture seam. Absent = the control is display-only.
    #[props(default)]
    on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let reset_label = label.clone();
    // Reset is a GESTURE, so it sits beside the label on the face rather
    // than inside the popup — one click, always.
    let reset = rsx! {
        if engaged && let Some(target) = target && let Some(handler) = on_panel {
            button {
                class: "tw:absolute tw:left-full tw:top-1/2 tw:ml-0.5 tw:inline-flex tw:flex-none tw:-translate-y-1/2 tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-status-engaged-foreground tw:opacity-70 tw:hover:opacity-100",
                r#type: "button",
                title: "Reset {reset_label} and let the project control it again",
                aria_label: "Reset {reset_label}",
                onclick: move |event| {
                    event.stop_propagation();
                    handler.call(PanelGesture::ClearControl {
                        target: target.clone(),
                    });
                },
                StudioIcon { name: StudioIconName::Revert, size: 10 }
            }
        }
    };
    rsx! {
        // `relative` so the engaged reset can hang off the label's right
        // edge WITHOUT occupying layout — its appearance must not reflow
        // the control (GV2).
        span { class: "tw:relative tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1",
            SlotDetailButton {
                label: label.clone(),
                aspects,
                name_override: Some(label.clone()),
                initially_open,
                placement: PopoverPlacement::BottomMiddle,
                anchor_id: Some(anchor_id),
                anchor_visual: Some(anchor_visual),
                trigger: Some(label_visual(&label, color_class)),
                trigger_class: LABEL_TRIGGER_CLASS.to_string(),
                trigger_open_class: LABEL_TRIGGER_CLASS.to_string(),
                on_action,
            }
            {reset}
        }
    }
}

/// The label visual shared by the trigger and its top-layer copy: the
/// control name in the panel's label typography plus the small info glyph
/// that stands for "this opens details", both in the state color.
pub(crate) fn label_visual(label: &str, color_class: &'static str) -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1 {color_class}",
            span { class: "tw:truncate tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em]",
                "{label}"
            }
            span { class: "tw:inline-flex tw:flex-none tw:opacity-60",
                StudioIcon { name: StudioIconName::InfoBare, size: 9 }
            }
        }
    }
}
