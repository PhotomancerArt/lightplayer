//! Raw-input detail popup for rich slot controls (the 1..m pattern).
//!
//! Rich controls (slider, XY pad) trade exactness for direct manipulation.
//! `SlotRawInputPopover` is the small affordance on those rows that opens a
//! detail card with exact numeric entry for the SAME slot path: the caller
//! passes the raw editor (a plain number field or component grid) as
//! children, wired to the same address and `on_action` conduit as the rich
//! control. Both views read the same DTO value and dispatch `SetValue` to
//! the same address — the path-keyed edit buffer keeps them coherent (ADR
//! D6, 1-value-to-many-controls). The rich control keeps `oninput`
//! semantics; raw inputs keep `onchange` (roadmap D5).

use dioxus::prelude::*;

use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, PopoverPlacement, StudioIconName,
    detail_popover_section_class,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotRawInputPopover(
    /// Accessible trigger label, e.g. "Exact value".
    #[props(default = "Exact value".to_string())]
    label: String,
    /// Open the popup on first render (stories).
    #[props(default = false)]
    initially_open: bool,
    children: Element,
) -> Element {
    rsx! {
        DetailPopover {
            icon: StudioIconName::Edited,
            label: label.clone(),
            title: label,
            tone: IconMenuTone::Quiet,
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            section { class: detail_popover_section_class(DetailSectionTint::None),
                header { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:leading-none",
                    h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:text-heading", "Exact value" }
                }
                div { class: "tw:flex tw:min-w-0 tw:justify-end tw:pt-1",
                    {children}
                }
            }
        }
    }
}
