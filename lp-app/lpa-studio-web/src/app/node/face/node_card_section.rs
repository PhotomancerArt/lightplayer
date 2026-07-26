//! The generic node-card section grammar (P2b item 1; single treatment
//! settled at the P2c re-gate).
//!
//! A face is ONE surface with divisions, not stacked widgets: every face
//! section (output, controls, agent, entries, code, advanced) renders
//! full-bleed inside the card, separated by 1px `border-muted` dividers —
//! no inner rounded or bordered boxes. An expanded section carries a slim
//! (20px) vertical label rail on its LEFT edge (the surviving variant A,
//! mirrored per the re-gate): rotated small-caps in the dim foreground
//! reading bottom-to-top like a book spine, an upright icon above; tint
//! appears only on hover of a toggleable rail so the rail never reads as
//! another box edge.
//!
//! Collapsed sections (drawers) render as a slim horizontal row — chevron +
//! label + mono summary hint. Sections without a toggle handler are
//! permanent (the face never leaves).

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeCardSection(
    /// Lowercase section label ("output", "controls", "agent", "entries",
    /// "code", "advanced").
    label: &'static str,
    /// Optional role icon beside the label (the agent section's sparkles).
    #[props(default = None)]
    icon: Option<StudioIconName>,
    /// One-line plain-language role subline (the agent affordance, P2b
    /// item 2).
    #[props(default = None)]
    subline: Option<&'static str>,
    /// Collapsed-row summary hint (drawers).
    #[props(default = None)]
    summary: Option<String>,
    /// `None` = permanent section (always expanded); `Some(open)` = drawer.
    #[props(default = None)]
    open: Option<bool>,
    /// Drawer toggle; the caller owns the open state.
    #[props(default = None)]
    on_toggle: Option<EventHandler<()>>,
    /// Suppress the top divider (the first section sits flush under the
    /// pane header's own border).
    #[props(default = false)]
    first: bool,
    /// Full-bleed section content — padding is the content's own business.
    children: Element,
) -> Element {
    let container = section_container_class(first);
    let toggleable = open.is_some() && on_toggle.is_some();
    let expanded = open.unwrap_or(true);

    if !expanded {
        return rsx! {
            section { class: container,
                CollapsedSectionRow { label, icon, summary, on_toggle }
            }
        };
    }

    rsx! {
        section { class: container,
            div { class: "tw:grid tw:min-w-0 tw:grid-cols-[20px_minmax(0,1fr)]",
                SectionRail { label, icon, toggleable, on_toggle }
                div { class: "tw:grid tw:min-w-0 tw:content-start",
                    if let Some(subline) = subline {
                        p { class: "tw:m-0 tw:px-4 tw:pt-2 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                            "{subline}"
                        }
                    }
                    {children}
                }
            }
        }
    }
}

/// Divider logic: every section but the first carries the 1px top divider.
fn section_container_class(first: bool) -> &'static str {
    if first {
        "tw:grid tw:min-w-0"
    } else {
        "tw:grid tw:min-w-0 tw:border-t tw:border-border-muted"
    }
}

/// The vertical label rail on the expanded section's LEFT edge: dim
/// small-caps reading bottom-to-top, an upright icon above, hover tint only
/// when the rail doubles as the drawer's collapse control.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SectionRail(
    label: &'static str,
    #[props(default = None)] icon: Option<StudioIconName>,
    #[props(default = false)] toggleable: bool,
    #[props(default = None)] on_toggle: Option<EventHandler<()>>,
) -> Element {
    let text = rsx! {
        if let Some(icon) = icon {
            StudioIcon { name: icon, size: 11 }
        }
        span {
            class: "tw:select-none tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.14em]",
            style: "writing-mode: vertical-rl; transform: rotate(180deg);",
            "{label}"
        }
    };

    if toggleable {
        let title = format!("Collapse {label}");
        rsx! {
            button {
                class: "tw:flex tw:cursor-pointer tw:appearance-none tw:flex-col tw:items-center tw:justify-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:py-2 tw:text-dim-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground",
                r#type: "button",
                aria_expanded: "true",
                aria_label: "{title}",
                title: "{title}",
                onclick: move |event| {
                    event.stop_propagation();
                    if let Some(handler) = on_toggle {
                        handler.call(());
                    }
                },
                {text}
            }
        }
    } else {
        rsx! {
            div { class: "tw:flex tw:flex-col tw:items-center tw:justify-center tw:gap-1.5 tw:py-2 tw:text-dim-foreground",
                {text}
            }
        }
    }
}

/// The collapsed drawer row: chevron, lowercase label, mono summary hint.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CollapsedSectionRow(
    label: &'static str,
    #[props(default = None)] icon: Option<StudioIconName>,
    #[props(default = None)] summary: Option<String>,
    #[props(default = None)] on_toggle: Option<EventHandler<()>>,
) -> Element {
    let title = format!("Expand {label}");

    rsx! {
        button {
            class: "tw:flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-4 tw:py-2 tw:text-left tw:text-xs tw:text-subtle-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground",
            r#type: "button",
            aria_expanded: "false",
            aria_label: "{title}",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_toggle {
                    handler.call(());
                }
            },
            StudioIcon { name: StudioIconName::Collapsed, size: 12 }
            if let Some(icon) = icon {
                StudioIcon { name: icon, size: 12 }
            }
            span { class: "tw:font-bold", "{label}" }
            if let Some(summary) = summary {
                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem]", "{summary}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::section_container_class;

    #[test]
    fn only_non_first_sections_carry_the_divider() {
        assert!(!section_container_class(true).contains("border-t"));
        assert!(section_container_class(false).contains("tw:border-t tw:border-border-muted"));
    }
}
