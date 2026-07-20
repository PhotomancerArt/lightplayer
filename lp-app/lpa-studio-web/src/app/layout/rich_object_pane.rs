//! The rich-object pane: the node pane's header treatment, codified.
//!
//! Q3 of the rich-object spike (P2 gate): the generalized pane header is
//! the REAL node header, not an approximation. The header *markup* is
//! [`StudioPane`] — the shared chrome grammar the node pane has always
//! rendered through — so this component adds no DOM of its own; it pins
//! the rich-object COMPOSITION the node pane established, so every
//! consumer gets the node look by construction:
//!
//! - the header wash is the object's rollup tone (worst-actionable
//!   section / merged affordance), not an arbitrary chrome choice;
//! - no header chips — under the P6 affordance model the detail trigger
//!   is the whole announcement, and counts/status words live in the
//!   detail popover;
//! - the detail popover slot sits at the header's right edge, its trigger
//!   styled by the same rollup (`affordance_trigger_style` /
//!   `status_trigger_style`).
//!
//! The node pane is the first consumer (pixel-identical — node story
//! baselines are the fidelity test); the M7 runtime pane renders the
//! device rich object through this same header.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPaneAction};

use crate::app::layout::{PaneChrome, PaneCollapse, PaneTone, StudioPane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RichObjectPane(
    /// Optional collapse rail (state + handler); the pane holds no state.
    #[props(default)]
    collapse: Option<PaneCollapse>,
    /// Primary affordance slot, left of the title (selection control,
    /// status indicator, …).
    #[props(default)]
    primary: Option<Element>,
    /// Pane title.
    title: String,
    /// Optional action dispatched when the title is activated.
    #[props(default)]
    title_action: Option<UiAction>,
    /// Optional kind/subtype text after the title.
    #[props(default)]
    kind: Option<String>,
    /// The object's rollup tone, washing the header strip (consumers map
    /// their merge — `affordance_pane_tone`, `RichObjectView::rollup` —
    /// onto the pane vocabulary).
    tone: PaneTone,
    /// Draw the pane outline in the neutral selection color (focus).
    #[props(default = false)]
    accent: bool,
    /// Contextual header actions rendered as icon buttons.
    #[props(default)]
    actions: Vec<UiPaneAction>,
    /// Action dispatch conduit for the actions slot.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    /// Free-form header extras between the actions and the detail popup.
    #[props(default)]
    trailing: Option<Element>,
    /// Detail-popup slot at the header's right edge (a `DetailPopover`
    /// whose trigger follows the same rollup).
    #[props(default)]
    detail: Option<Element>,
    /// Pane body; `None` renders a header-only pane.
    #[props(default)]
    body: Option<Element>,
) -> Element {
    rsx! {
        StudioPane {
            collapse,
            primary,
            title,
            title_action,
            kind,
            chrome: PaneChrome {
                tone,
                accent,
                // P6: no header chips — the affordance-following detail
                // trigger is the whole announcement.
                chips: Vec::new(),
            },
            actions,
            on_action,
            trailing,
            detail,
            body,
        }
    }
}
