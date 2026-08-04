//! The generic node-card section grammar (P2b item 1; single treatment
//! settled at the P2c re-gate, legibility pass from the live review).
//!
//! A face is ONE surface with divisions, not stacked widgets: every face
//! section (output, controls, agent, entries, code, advanced) renders
//! full-bleed inside the card, separated by 1px `border-strong` hairlines —
//! no inner rounded or bordered boxes. (The original `border-muted`
//! dividers rendered but sat below the perceptual threshold against the
//! card surface on real displays; `border-strong` is the same light
//! hairline family the knob track and fader slot already wear.)
//!
//! An expanded section carries a slim (20px) vertical label rail on its
//! LEFT edge: rotated small-caps reading bottom-to-top like a book spine,
//! an upright icon above. The rail and the collapsed drawer row are TWO
//! STATES OF ONE CONTROL and share the same label typography and the same
//! chevron glyph: the collapsed row's chevron points right (closed) and
//! rotates toward down on hover; a toggleable rail wears the down chevron
//! (open) at its top, dimmed at rest, and rotates it toward right on hover
//! — each state previews the other. Rails without a toggle handler are
//! permanent pure labels: no chevron, no hover tint, dimmer text.

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

/// Shared label typography for the section grammar's two states (vertical
/// rail and collapsed row) — same face, different writing mode.
const SECTION_LABEL_TEXT_CLASS: &str = "tw:select-none tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.14em]";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeCardSection(
    /// Lowercase section label ("output", "settings", "panel", "agent",
    /// "entries", "code", "advanced").
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
    /// The module PANEL's teaching treatment: a `panel-primary` wash over
    /// the section and a heading-toned rail, marking the one section that
    /// is the performable product surface — the thing play mode renders
    /// (spike gate 2; ADR 2026-08-04-wiring-flow-and-panel-settings).
    #[props(default = false)]
    panel_tint: bool,
    /// Full-bleed section content — padding is the content's own business.
    children: Element,
) -> Element {
    let container = section_container_class(first, panel_tint);
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

/// Divider logic: every section but the first carries the 1px top hairline
/// (`border-strong` — `border-muted` was invisible against the card in the
/// wired app).
fn section_container_class(first: bool, panel_tint: bool) -> String {
    let mut class = String::from(if first {
        "tw:grid tw:min-w-0"
    } else {
        "tw:grid tw:min-w-0 tw:border-t tw:border-border-strong"
    });
    if panel_tint {
        class.push_str(
            " tw:bg-[linear-gradient(90deg,rgba(24,32,29,0.85),rgba(24,32,29,0.25)_60%,transparent)]",
        );
    }
    class
}

/// The vertical label rail on the expanded section's LEFT edge. A
/// toggleable rail is the drawer's collapse control and says so: the shared
/// chevron at its top (down = open, dimmed at rest) plus hover tint; hover
/// rotates the chevron toward the collapsed row's right-pointing state.
/// Non-toggleable rails are pure labels: no chevron, no hover, dim text.
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
            class: SECTION_LABEL_TEXT_CLASS,
            style: "writing-mode: vertical-rl; transform: rotate(180deg);",
            "{label}"
        }
    };

    if toggleable {
        let title = format!("Collapse {label}");
        rsx! {
            button {
                class: "tw:group tw:flex tw:cursor-pointer tw:appearance-none tw:flex-col tw:items-center tw:justify-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:py-2 tw:text-subtle-foreground tw:transition-colors tw:hover:bg-card-muted tw:hover:text-strong-foreground tw:motion-reduce:transition-none",
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
                // The shared drawer chevron in its OPEN rotation, dimmed at
                // rest; hover previews the collapsed state (points right).
                span { class: "tw:inline-flex tw:opacity-40 tw:transition-[opacity,transform] tw:duration-150 tw:group-hover:-rotate-90 tw:group-hover:opacity-100 tw:motion-reduce:transition-none",
                    StudioIcon { name: StudioIconName::Expanded, size: 12 }
                }
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

/// The collapsed drawer row: the shared chevron in its CLOSED rotation
/// (rotating toward down on hover — the expanded state it leads to), the
/// same label typography as the rail, and a mono summary hint.
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
            class: "tw:group tw:flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-4 tw:py-2 tw:text-left tw:text-subtle-foreground tw:transition-colors tw:hover:bg-card-muted tw:hover:text-strong-foreground tw:motion-reduce:transition-none",
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
            span { class: "tw:inline-flex tw:transition-transform tw:duration-150 tw:group-hover:rotate-90 tw:motion-reduce:transition-none",
                StudioIcon { name: StudioIconName::Collapsed, size: 12 }
            }
            if let Some(icon) = icon {
                StudioIcon { name: icon, size: 11 }
            }
            span { class: SECTION_LABEL_TEXT_CLASS, "{label}" }
            if let Some(summary) = summary {
                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:normal-case tw:tracking-normal tw:text-dim-foreground",
                    "{summary}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SECTION_LABEL_TEXT_CLASS, section_container_class};

    #[test]
    fn only_non_first_sections_carry_the_divider() {
        assert!(!section_container_class(true, false).contains("border-t"));
        assert!(
            section_container_class(false, false).contains("tw:border-t tw:border-border-strong")
        );
    }

    #[test]
    fn the_panel_tint_is_opt_in() {
        // The module PANEL's teaching wash (spike gate 2) must never leak
        // onto ordinary sections.
        assert!(section_container_class(false, true).contains("linear-gradient"));
        assert!(!section_container_class(false, false).contains("linear-gradient"));
    }

    #[test]
    fn rail_and_collapsed_row_share_one_label_typography() {
        // The two section states must read as one control: the shared
        // constant is the contract (uppercase small-caps, same size and
        // tracking in both writing modes).
        for expectation in [
            "uppercase",
            "text-[0.6rem]",
            "tracking-[0.14em]",
            "font-bold",
        ] {
            assert!(
                SECTION_LABEL_TEXT_CLASS.contains(expectation),
                "label typography lost {expectation}"
            );
        }
    }
}
