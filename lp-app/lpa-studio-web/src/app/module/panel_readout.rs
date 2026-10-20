//! [`PanelReadout`]: the one value a panel control shows, wherever a panel
//! draws it (the editor's and the play page's
//! [`super::ModulePanelControl`], the board card's [`super::PanelCell`] and
//! [`super::PanelStrip`]).
//!
//! In every state the face carries exactly ONE number
//! ([`UiPanelControl::shown_display`]): the live reading when the channel has
//! one, the authored value when it does not. A phasor's period reads in
//! seconds, the Speed knob's rate as a speed (`2×`), a palette as its compact
//! chip (`5 stops`, `↻ 4 · 20 s`, the full summary on hover).
//!
//! Values render in **tabular numerals inside a reserved slot**, because a
//! panel whose row reflows while you drag a fader is not a control panel.

use dioxus::prelude::*;
use lpa_studio_core::app::project::format_gradient_chip;
use lpa_studio_core::{UiPanelControl, UiPanelEmit, UiPanelWidget, UiSlotValueKind};

use crate::app::node::{SlotUnitSuffix, speed_words};

/// The value slot: tabular numerals in a reserved box, so 0.6 / 0.62 / 200
/// all occupy the same width and nothing moves during a drag.
const READOUT_CLASS: &str = "tw:inline-flex tw:min-w-[4.5ch] tw:flex-none tw:items-baseline tw:justify-center tw:gap-1 tw:font-mono tw:text-[0.7rem] tw:tabular-nums";

/// The control's one value, in its state's colour (`color_class`, from
/// [`super::panel_state_look`]). `class` adds the surface's own layout.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelReadout(
    control: UiPanelControl,
    color_class: &'static str,
    #[props(default)] class: &'static str,
) -> Element {
    let shown_value = shown_value(&control);
    // Hovering a palette keeps the full summary reachable — the chip
    // deliberately drops the space, method, and fade the dense line states.
    let readout_title = match control.shown_palette() {
        Some(_) => control
            .live_value
            .clone()
            .unwrap_or_else(|| control.value.display.clone()),
        None => String::new(),
    };
    rsx! {
        span { class: "{READOUT_CLASS} {color_class} {class}",
            span { title: readout_title, "{shown_value}" }
            SlotUnitSuffix { unit: control.unit.clone(), reserve: false }
        }
    }
}

/// The words of the control's one value.
fn shown_value(control: &UiPanelControl) -> String {
    // A phasor speed knob presents its stored period as a reciprocal and
    // drags on an inverted axis (up = faster); it reads in seconds.
    if matches!(control.emit, UiPanelEmit::PhasorPeriod { .. }) {
        return format!("{} s", control.shown_display());
    }
    // The clock's Speed knob reads as a speed (`2×`).
    if matches!(control.widget, UiPanelWidget::Speed) {
        return speed_words(control.live_numeric().unwrap_or(match control.value.kind {
            UiSlotValueKind::F32(rate) => rate,
            _ => 1.0,
        }));
    }
    // A palette reads as its compact chip; the strips say WHICH palette.
    if let Some(config) = control.shown_palette() {
        return format_gradient_chip(&config);
    }
    control.shown_display().to_string()
}
