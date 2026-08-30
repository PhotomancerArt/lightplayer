//! The card-resident modal sheet (D41): THE destructive-confirm pattern
//! for card-resident actions, and the shape the name-stamping and D30
//! drift sheets ride.
//!
//! A sheet dims the card within its own footprint and SPARES the title
//! bar (ratified spike round 3: "keep the title visible to see what
//! device it is") — structurally, by mounting inside the card's
//! below-the-header wrapper rather than measuring. The tint edge stays
//! visible (the overlay sits inside the card's border). Click outside
//! the panel dismisses; the panel is a compact centered column.
//!
//! This replaces the native `confirm()` treatment for card-resident
//! actions only — non-card surfaces (node panes, toolbars)
//! keep the [`ActionButton`](crate::core::ActionButton) confirm path.

use dioxus::prelude::*;

use crate::core::{quiet_action_class, quiet_destructive_action_class};

/// The dim-within-the-card overlay + centered panel. Content is the
/// caller's; the sheet owns dimming, placement, and outside-click
/// dismissal.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardSheet(on_dismiss: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "ux-card-sheet",
            onclick: move |event| {
                // only the backdrop dismisses; panel clicks stay inside
                event.stop_propagation();
                on_dismiss.call(());
            },
            div {
                class: "ux-card-sheet-panel ux-glass-panel",
                onclick: move |event| event.stop_propagation(),
                {children}
            }
        }
    }
}

/// The sheet's title row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardSheetTitle(text: String) -> Element {
    rsx! {
        h3 { class: "tw:m-0 tw:mb-2 tw:text-xs tw:font-bold tw:text-strong-foreground", "{text}" }
    }
}

/// The sheet's body copy.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardSheetMessage(text: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:mb-3 tw:text-xs tw:leading-normal tw:text-muted-foreground", "{text}" }
    }
}

/// The sheet's button row (right-aligned, cancel first by convention).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardSheetButtons(children: Element) -> Element {
    rsx! {
        div { class: "tw:flex tw:justify-end tw:gap-2", {children} }
    }
}

/// One sheet button. `tone` picks the chrome: quiet cancel, primary
/// (good family), or destructive (error family).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SheetButtonTone {
    Quiet,
    Primary,
    Destructive,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardSheetButton(
    label: String,
    tone: SheetButtonTone,
    onclick: EventHandler<()>,
) -> Element {
    // Quiet/Destructive fold into the shared quiet-chip family (P4); Primary
    // stays local — a "good"-toned confirm has no existing canonical
    // helper, and this is its only caller.
    let class = match tone {
        SheetButtonTone::Quiet => quiet_action_class().to_string(),
        SheetButtonTone::Primary => {
            "tw:cursor-pointer tw:rounded-md tw:border tw:border-[var(--studio-status-good-text)] tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-semibold tw:text-[var(--studio-status-good-text)]"
                .to_string()
        }
        SheetButtonTone::Destructive => quiet_destructive_action_class().to_string(),
    };
    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}
