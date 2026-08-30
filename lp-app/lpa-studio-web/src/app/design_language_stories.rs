//! The design-language page — the design library's MAIN page (it is the
//! story-book's `DEFAULT_STORY_ID`): the visual twin of `docs/style/ui.md`
//! Color & Light. Every idiom is shown with the REAL classes and tokens,
//! never a restatement, so this page drifts only when the language does.
//!
//! Ordering mirrors ui.md: the ground, frozen status meaning, kinds of
//! light, the selection grammar (ADR 2026-08-30-studio-design-language-
//! aurora, selection-grammar amendment), working spectrum, glass + focus.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{
    OPTION_CARD_CHECK_CLASS, conic_spinner_class, ir_ring_class, iridescent_fill_class,
    option_card_class, option_card_grid_class, row_edge_class,
};

/// One labeled section: title, the rule in one line, then the demo row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn Idiom(title: &'static str, rule: &'static str, children: Element) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            h3 { class: "tw:m-0 tw:text-sm tw:font-extrabold tw:text-heading", "{title}" }
            p { class: "tw:m-0 tw:max-w-[68ch] tw:text-xs tw:leading-relaxed tw:text-subtle-foreground",
                "{rule}"
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-3", {children} }
        }
    }
}

/// A status-family chip: the family's own ladder (tinted bg, mid border,
/// bright text), captioned with its one frozen meaning.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StatusChip(label: &'static str, meaning: &'static str, style: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-1 tw:justify-items-center",
            span {
                class: "tw:rounded-sm tw:border tw:px-2 tw:py-1 tw:font-mono tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.08em]",
                style: "{style}",
                "{label}"
            }
            span { class: "tw:text-[10px] tw:text-dim-foreground", "{meaning}" }
        }
    }
}

fn status_style(family: &str) -> String {
    format!(
        "background: var(--studio-status-{family}-bg); border-color: var(--studio-status-{family}-border); color: var(--studio-status-{family}-text);"
    )
}

#[story(
    label = "page",
    description = "The Studio design language on one page: Aurora ground, frozen status families, kinds of light, the selection grammar, the working spectrum, glass and focus."
)]
fn page() -> Element {
    let demo_chip = "tw:rounded-md tw:border tw:border-border tw:bg-card-raised tw:px-3 tw:py-1.5 tw:text-xs tw:text-muted-foreground";
    let mini_tab =
        "tw:relative tw:cursor-default tw:px-3 tw:py-2 tw:text-sm tw:font-bold tw:tracking-tight";
    let mini_nav_row = "tw:relative tw:cursor-default tw:rounded-sm tw:border tw:border-transparent tw:px-2.5 tw:py-1 tw:text-xs";
    rsx! {
        article { class: "tw:grid tw:max-w-[860px] tw:min-w-0 tw:gap-7 tw:p-1",
            header { class: "tw:grid tw:gap-1.5",
                h2 { class: "tw:m-0 tw:text-lg tw:font-extrabold tw:text-strong-foreground",
                    "The Studio design language"
                }
                p { class: "tw:m-0 tw:max-w-[70ch] tw:text-[13px] tw:leading-relaxed tw:text-soft-foreground",
                    "Aurora: violet-tinted graphite at rest — saturated color belongs to the artwork, "
                    "to status meaning, and to interaction light. There is no accent hue. The prose "
                    "version of this page is docs/style/ui.md § Color & Light."
                }
            }

            Idiom {
                title: "Status hues are meaning, and they are frozen",
                rule: "Each family means exactly one thing; chrome may never borrow its color. A new feature that wants \"a color\" reaches for a neutral or the spectrum, never a status hue.",
                StatusChip { label: "bus", meaning: "binding", style: status_style("bound") }
                StatusChip { label: "edit", meaning: "unsaved", style: status_style("warning") }
                StatusChip { label: "attention", meaning: "device health", style: status_style("attention") }
                StatusChip { label: "engaged", meaning: "hand on control", style: status_style("engaged") }
                StatusChip { label: "live", meaning: "running now", style: status_style("live") }
                StatusChip { label: "export", meaning: "ships from here", style: status_style("export") }
                StatusChip { label: "example", meaning: "read-only example", style: status_style("example") }
                StatusChip { label: "valid", meaning: "good", style: status_style("good") }
                div { class: "tw:grid tw:gap-1 tw:justify-items-center",
                    span {
                        class: "tw:rounded-sm tw:border tw:px-2 tw:py-1 tw:font-mono tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.08em]",
                        style: "background-color: var(--studio-status-error-bg); background-image: var(--studio-status-error-stripes); border-color: var(--studio-status-error-border); color: var(--studio-status-error-text);",
                        "error"
                    }
                    span { class: "tw:text-[10px] tw:text-dim-foreground", "stripes = not ordinary" }
                }
            }

            Idiom {
                title: "Moving light belongs to the pointer",
                rule: "The iridescent ring answers hover, the flare answers press, the pinned ring plus a lifted shadow rides a drag. Dense rows take the light variant: a blue edge plus bloom, never a ring per row. Motion is EXCLUSIVELY transient pointer state — nothing at rest may animate.",
                button { class: "{demo_chip} {ir_ring_class()} ux-press-flare tw:cursor-pointer", "hover / press me" }
                div { class: "tw:grid tw:w-44 tw:gap-1",
                    div { class: "{row_edge_class()} tw:cursor-default tw:rounded-sm tw:px-2.5 tw:py-1.5 tw:text-xs tw:text-soft-foreground tw:hover:bg-card-muted",
                        "a dense row (hover me)"
                    }
                }
                div { class: "{demo_chip} ux-drag-chip", "mid-drag" }
            }

            Idiom {
                title: "The selection grammar: you-are-here is a line",
                rule: "Selection and navigation are separate concepts and never share a mark. Nav's mark is a STATIC spectrum line on the nav axis's edge — full rainbow on the large tab underline, cool sweep on small side lines — so a place never looks like a chosen thing, and static light never blurs with the moving hover light.",
                div { class: "tw:flex tw:items-center tw:rounded-md tw:border tw:border-border-muted tw:bg-background tw:px-2",
                    span { class: "{mini_tab} ux-here-line-x tw:text-heading", "Nodes" }
                    span { class: "{mini_tab} tw:text-subtle-foreground", "Map" }
                    span { class: "{mini_tab} tw:text-subtle-foreground", "Patch" }
                }
                div { class: "tw:grid tw:w-44 tw:gap-0.5 tw:rounded-md tw:border tw:border-border-muted tw:bg-card tw:p-1.5",
                    span { class: "{mini_nav_row} tw:text-muted-foreground", "plasma.glsl" }
                    span { class: "{mini_nav_row} ux-here-line-y tw:bg-[linear-gradient(90deg,var(--studio-color-selection-bg),transparent_90%)] tw:text-strong-foreground",
                        "Ocean Ripple"
                    }
                    span { class: "{mini_nav_row} tw:text-muted-foreground", "dusk palette" }
                }
            }

            Idiom {
                title: "The selection grammar: a chosen object wears the ring",
                rule: "Object selection — an option card, a future tree multi-select — is a STATIC spectrum ring over the neutral selection wash and check. Full spectrum at card radius; the cool variant at small radii, where the full sweep compresses to its warm stops and reads as attention-orange.",
                div { class: "{option_card_grid_class()} tw:w-72",
                    span { class: option_card_class(true),
                        span { class: OPTION_CARD_CHECK_CLASS, "✓" }
                        span { class: "tw:text-[11.5px] tw:font-medium", "chosen" }
                        span { class: "tw:text-[10px] tw:text-dim-foreground", "static spectrum ring" }
                    }
                    span { class: option_card_class(false),
                        span { class: "tw:text-[11.5px] tw:font-medium", "plain" }
                        span { class: "tw:text-[10px] tw:text-dim-foreground", "neutral at rest" }
                    }
                }
            }

            Idiom {
                title: "The working spectrum",
                rule: "In-flight work sweeps the spectrum: the conic spinner, the iridescent progress fill, and the gradient Primary — the one loud fill in the app. Progress is never a flat colored bar.",
                span { class: conic_spinner_class() }
                div { class: "tw:h-2 tw:w-40 tw:overflow-hidden tw:rounded-pill tw:bg-track",
                    div { class: "{iridescent_fill_class()} tw:h-full tw:w-3/5 tw:rounded-pill" }
                }
                button { class: "ux-primary-gradient ux-press-flare tw:cursor-pointer tw:rounded-md tw:border tw:px-3.5 tw:py-1.5 tw:text-xs tw:font-bold",
                    "Primary"
                }
            }

            Idiom {
                title: "Glass floats; focus is never optional",
                rule: "Glass is for overlays only — popovers, sheets, bars above the canvas; resting surfaces stay opaque (glass under glass reads as mud). Every control a mouse can reach shows the focus ring to a keyboard.",
                div { class: "ux-glass-panel tw:rounded-md tw:px-3 tw:py-2 tw:text-xs tw:text-soft-foreground",
                    "an overlay"
                }
                input {
                    class: "tw:w-36 tw:rounded-sm tw:border tw:border-border tw:bg-card-subtle tw:px-2 tw:py-1 tw:text-xs tw:text-soft-foreground",
                    placeholder: "tab to me",
                }
            }
        }
    }
}
