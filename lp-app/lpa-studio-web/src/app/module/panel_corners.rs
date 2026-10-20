//! [`PanelCorners`]: a cell's or a strip's two corner actions on the
//! connected board card's panel — let go (↺) top-left, details (ⓘ)
//! top-right — as 24 px targets holding 22 px circles, 4 px in from the top
//! corners (the panel layout study, `docs/ux/studies/panel-layout/`; the
//! guide's proposed P21, "targets fit the hand").
//!
//! They float over the control (`style.css` `.ux-panel-corner`) and never
//! move its label or its value. ↺ shows only while a panel write holds the
//! control; it sends [`PanelGesture::ClearControl`] through the panel's own
//! gesture seam, as the editor's glyph does. ⓘ is the details popover's
//! trigger: on hover, while the control has keyboard focus, while its
//! details are open, and always on a toggle where nothing hovers (its tap
//! is its flip). The cell owns the popover's open state, so a tap on the
//! control opens the same details.
//!
//! The circles are siblings of the control's gesture surface, never inside
//! it (a toggle's surface is a button, and a button holds no other), so a
//! press on one never reaches the surface.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPanelTarget, UiSlotAspect};

use crate::app::node::SlotDetailButton;
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName};

use super::PanelGesture;

/// The let-go target: 24 px, its circle inside.
const LET_GO_CLASS: &str = "ux-panel-corner ux-panel-corner-let-go tw:cursor-pointer tw:appearance-none tw:rounded-full tw:border-0 tw:bg-transparent tw:p-0 tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-focus-ring";

/// The let-go circle: the engaged family, as the held control it lets go.
const LET_GO_CIRCLE_CLASS: &str = "tw:grid tw:h-[22px] tw:w-[22px] tw:place-items-center tw:rounded-full tw:border tw:border-status-engaged-border tw:bg-status-engaged-bg tw:text-status-engaged-foreground";

/// The details trigger: 24 px, its circle inside.
const DETAILS_TRIGGER_CLASS: &str = "tw:grid tw:h-6 tw:w-6 tw:cursor-pointer tw:appearance-none tw:place-items-center tw:rounded-full tw:border-0 tw:bg-transparent tw:p-0 tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-focus-ring";

/// The details circle: quiet chrome, raised a step off the panel.
const DETAILS_CIRCLE_CLASS: &str = "tw:grid tw:h-[22px] tw:w-[22px] tw:place-items-center tw:rounded-full tw:border tw:border-border-strong tw:bg-card-raised tw:text-muted-foreground";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelCorners(
    /// The control's label: the details' title and both circles' names.
    label: String,
    /// The details popup's sections.
    aspects: Vec<UiSlotAspect>,
    /// The id of the cell or strip the details' outline welds to.
    anchor_id: String,
    /// The control's live copy, painted over the anchor while open.
    anchor_visual: Element,
    /// The details' open state, owned by the cell (a tap sets it).
    open: Signal<bool>,
    /// Whether a panel write holds the control: only then is ↺ drawn.
    engaged: bool,
    /// What ↺ clears. A fixture control without one draws no ↺, even when
    /// staged as engaged.
    target: Option<UiPanelTarget>,
    /// The panel's gesture seam.
    on_panel: EventHandler<PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let let_go_label = label.clone();
    let details_label = label.clone();
    rsx! {
        if engaged && let Some(target) = target {
            button {
                class: LET_GO_CLASS,
                r#type: "button",
                title: "Reset {let_go_label} and let the project control it again",
                aria_label: "Reset {let_go_label}",
                "data-panel-let-go": "",
                onclick: move |event| {
                    event.stop_propagation();
                    on_panel.call(PanelGesture::ClearControl {
                        target: target.clone(),
                    });
                },
                span { class: LET_GO_CIRCLE_CLASS,
                    StudioIcon { name: StudioIconName::Revert, size: 12 }
                }
            }
        }
        span { class: "ux-panel-corner ux-panel-corner-details", "data-panel-details": "",
            SlotDetailButton {
                label: details_label,
                aspects,
                name_override: Some(label),
                initially_open: open(),
                open_signal: Some(open),
                placement: PopoverPlacement::BottomMiddle,
                anchor_id: Some(anchor_id),
                anchor_visual: Some(anchor_visual),
                trigger: Some(rsx! {
                    span { class: DETAILS_CIRCLE_CLASS,
                        StudioIcon { name: StudioIconName::InfoBare, size: 11 }
                    }
                }),
                trigger_class: DETAILS_TRIGGER_CLASS.to_string(),
                trigger_open_class: DETAILS_TRIGGER_CLASS.to_string(),
                on_action: Some(on_action),
            }
        }
    }
}
