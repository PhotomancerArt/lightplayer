//! [`panel_label`]: a panel control's name in the panel's label
//! typography, in its state's colour — the plain label the board card's
//! cells and strips draw inside their gesture surface (their details open
//! from the corner ⓘ or a tap, so the label is no trigger and wears no
//! glyph).

use dioxus::prelude::*;

/// The control's name: small, bold, uppercase, cut at the cell's width.
pub(crate) fn panel_label(label: &str, color_class: &'static str) -> Element {
    rsx! {
        span { class: "tw:min-w-0 tw:max-w-full tw:truncate tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] {color_class}",
            "{label}"
        }
    }
}
