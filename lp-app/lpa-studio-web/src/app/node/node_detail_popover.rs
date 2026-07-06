//! Node detail affordance: the always-visible status icon opening the node's
//! one detail popover — today's status content plus a dirty section (counts
//! by kind) when the node's subtree carries edits.
//!
//! Rendered into the node pane's detail slot at the header's right edge; the
//! header layout itself is the shared `StudioPane` component.

use dioxus::prelude::*;
use lpa_studio_core::UiNodeHeader;
use lpa_studio_core::core::status::UiStatusKind;

use crate::app::layout::PaneTone;
use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, StudioIconName, detail_popover_section_class,
};

/// Map a node status kind onto the pane's neutral tone vocabulary (the
/// consumer-side mapping required by the pane's layout-only contract).
pub(crate) fn status_pane_tone(kind: UiStatusKind) -> PaneTone {
    match kind {
        UiStatusKind::Neutral => PaneTone::Neutral,
        UiStatusKind::Working => PaneTone::Working,
        UiStatusKind::Good => PaneTone::Good,
        UiStatusKind::Warning => PaneTone::Warning,
        UiStatusKind::Error => PaneTone::Error,
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn NodeDetailPopover(
    header: UiNodeHeader,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let dirty = header.dirty;
    let label = format!("{} details", header.title);

    rsx! {
        DetailPopover {
            icon: status_icon(header.status.kind),
            label,
            tone: status_menu_tone(header.status.kind),
            initially_open,
            section { class: detail_popover_section_class(DetailSectionTint::None),
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4 tw:py-1",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{header.title}" }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "{header.kind}" }
                    }
                    span { class: node_status_label_class(header.status.kind), "{header.status.label}" }
                }
            }
            section { class: detail_popover_section_class(DetailSectionTint::None),
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1.5 tw:py-1 tw:text-xs",
                    if let Some(summary) = header.summary.as_ref() {
                        NodeDetailRow { label: "summary", value: summary.clone() }
                    }
                    if let Some(source) = header.source.as_ref() {
                        NodeDetailRow { label: "source", value: source.clone() }
                    }
                    NodeDetailRow { label: "path", value: header.path.clone() }
                }
            }
            if let Some(detail) = header.detail.as_ref() {
                section { class: detail_popover_section_class(DetailSectionTint::None),
                    p { class: "tw:m-0 tw:py-1 tw:text-xs tw:leading-normal tw:text-muted-foreground tw:break-words", "{detail}" }
                }
            }
            if dirty.persisted > 0 {
                section { class: detail_popover_section_class(DetailSectionTint::Warning),
                    NodeDirtyCountRow { label: "Unsaved (persisted)", count: dirty.persisted }
                }
            }
            if dirty.transient > 0 {
                section { class: detail_popover_section_class(DetailSectionTint::Live),
                    NodeDirtyCountRow { label: "Live (transient)", count: dirty.transient }
                }
            }
            if dirty.failed > 0 {
                section { class: detail_popover_section_class(DetailSectionTint::Error),
                    NodeDirtyCountRow { label: "Failed edits", count: dirty.failed }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[72px_minmax(0,1fr)] tw:gap-2",
            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words", "{value}" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeDirtyCountRow(label: &'static str, count: usize) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono tw:text-muted-foreground", "{count}" }
        }
    }
}

/// Detail-trigger glyph discipline (UX gate): the default trigger is the
/// "i" info glyph matching slot rows — a Good/running status must not render
/// a play triangle that reads as a button. Only genuinely attention-needing
/// states (Warning/Error) keep their status glyphs; the status still shows
/// through the trigger's tone.
fn status_icon(kind: UiStatusKind) -> StudioIconName {
    match kind {
        UiStatusKind::Neutral | UiStatusKind::Working | UiStatusKind::Good => {
            StudioIconName::InfoBare
        }
        UiStatusKind::Warning => StudioIconName::StepAttention,
        UiStatusKind::Error => StudioIconName::StatusError,
    }
}

/// Map a node status kind onto the icon-menu trigger tone so the status icon
/// keeps its status coloring as the detail-popup trigger.
fn status_menu_tone(kind: UiStatusKind) -> IconMenuTone {
    match kind {
        UiStatusKind::Neutral => IconMenuTone::Neutral,
        UiStatusKind::Working => IconMenuTone::Working,
        UiStatusKind::Good => IconMenuTone::Good,
        UiStatusKind::Warning => IconMenuTone::Warning,
        UiStatusKind::Error => IconMenuTone::Error,
    }
}

fn node_status_label_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-neutral-foreground"
        }
        UiStatusKind::Working => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-working-foreground"
        }
        UiStatusKind::Good => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-good-foreground"
        }
        UiStatusKind::Warning => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-warning-foreground"
        }
        UiStatusKind::Error => {
            "tw:shrink-0 tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:leading-none tw:text-status-error-foreground"
        }
    }
}
