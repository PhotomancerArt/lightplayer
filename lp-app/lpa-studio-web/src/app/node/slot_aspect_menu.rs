//! Unified aspect menu and row treatment for config slots.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};

use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotAspectMenu(label: String, aspects: Vec<UiSlotAspect>) -> Element {
    let affordance = primary_affordance(&aspects);
    let style = slot_affordance_style(affordance);
    let menu_label = format!("{label} details");

    rsx! {
        span { class: "tw:inline-flex tw:w-6 tw:justify-end",
            IconMenuButton {
                icon: style.icon,
                icon_size: 13,
                label: menu_label.clone(),
                title: menu_label,
                tone: style.tone,
                placement: PopoverPlacement::BottomEnd,
                active: style.active,
                popup_class: slot_aspect_popup_class().to_string(),
                div { class: "tw:min-w-0 tw:px-3 tw:py-2",
                    strong { class: "tw:block tw:text-sm tw:text-strong-foreground tw:break-words", "{label}" }
                }
                for aspect in aspects {
                    SlotAspectSection { aspect }
                }
            }
        }
    }
}

pub(crate) fn slot_row_class(affordance: UiSlotAffordance, index: usize) -> &'static str {
    match affordance {
        UiSlotAffordance::Error | UiSlotAffordance::Invalid => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-error-bg)_0%,var(--studio-status-error-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Edited => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-warning-bg)_0%,var(--studio-status-warning-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Saving => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-working-bg)_0%,var(--studio-status-working-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Bound => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-good-bg)_0%,var(--studio-status-good-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Info if index % 2 == 0 => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-color-surface-muted)_0%,var(--studio-color-surface-muted)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Info => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-color-surface-subtle)_0%,var(--studio-color-surface-subtle)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
    }
}

pub(crate) fn primary_affordance(aspects: &[UiSlotAspect]) -> UiSlotAffordance {
    aspects
        .iter()
        .filter_map(|aspect| aspect.affordance)
        .max()
        .unwrap_or(UiSlotAffordance::Info)
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotAspectSection(aspect: UiSlotAspect) -> Element {
    let summary = aspect_summary(&aspect);
    let section_class = aspect_section_class(summary.highlight);
    let heading_class = aspect_heading_class(summary.tone);
    let icon_class = aspect_icon_class(summary.tone);
    let details = aspect_detail_lines(&aspect);
    let info_rows = if aspect.kind == UiSlotAspectKind::TypeInfo {
        aspect.rows.clone()
    } else {
        Vec::new()
    };

    rsx! {
        section { class: section_class,
            header { class: "tw:flex tw:items-center tw:gap-1.5 tw:leading-none",
                span { class: icon_class,
                    StudioIcon {
                        name: summary.icon,
                        size: 12,
                    }
                }
                h3 { class: heading_class, "{summary.title}" }
            }
            if aspect.kind == UiSlotAspectKind::TypeInfo {
                div { class: "tw:grid tw:gap-0.5 tw:pl-[18px]",
                    for row in info_rows {
                        SlotInfoRow { row }
                    }
                }
            } else if !details.is_empty() {
                div { class: "tw:grid tw:gap-0.5 tw:pl-[18px]",
                    for detail in details {
                        p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground tw:break-words", "{detail}" }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SlotAffordanceStyle {
    tone: IconMenuTone,
    icon: StudioIconName,
    active: bool,
}

#[derive(Clone, Copy)]
struct AspectSummary {
    title: &'static str,
    icon: StudioIconName,
    tone: AspectTone,
    highlight: Option<UiSlotAffordance>,
}

#[derive(Clone, Copy)]
enum AspectTone {
    Quiet,
    Good,
    Accent,
    Working,
    Warning,
    Error,
}

fn slot_affordance_style(affordance: UiSlotAffordance) -> SlotAffordanceStyle {
    match affordance {
        UiSlotAffordance::Info => SlotAffordanceStyle {
            tone: IconMenuTone::Quiet,
            icon: StudioIconName::Info,
            active: true,
        },
        UiSlotAffordance::Saving => SlotAffordanceStyle {
            tone: IconMenuTone::Working,
            icon: StudioIconName::StatusRunning,
            active: true,
        },
        UiSlotAffordance::Bound => SlotAffordanceStyle {
            tone: IconMenuTone::Accent,
            icon: StudioIconName::BoundValue,
            active: true,
        },
        UiSlotAffordance::Edited => SlotAffordanceStyle {
            tone: IconMenuTone::Warning,
            icon: StudioIconName::Edited,
            active: true,
        },
        UiSlotAffordance::Invalid => SlotAffordanceStyle {
            tone: IconMenuTone::Error,
            icon: StudioIconName::StepAttention,
            active: true,
        },
        UiSlotAffordance::Error => SlotAffordanceStyle {
            tone: IconMenuTone::Error,
            icon: StudioIconName::StatusError,
            active: true,
        },
    }
}

fn slot_aspect_popup_class() -> &'static str {
    "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:gap-0 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:text-sm tw:text-muted-foreground tw:shadow-lg"
}

fn aspect_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.kind {
        UiSlotAspectKind::Validation => validation_summary(aspect),
        UiSlotAspectKind::EditState => edit_state_summary(aspect),
        UiSlotAspectKind::Binding => binding_summary(aspect),
        UiSlotAspectKind::TypeInfo => AspectSummary {
            title: "Info",
            icon: StudioIconName::Info,
            tone: AspectTone::Quiet,
            highlight: None,
        },
    }
}

fn validation_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Error) => AspectSummary {
            title: "Error",
            icon: StudioIconName::StatusError,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Error),
        },
        Some(UiSlotAffordance::Invalid) => AspectSummary {
            title: "Invalid",
            icon: StudioIconName::StepAttention,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Invalid),
        },
        _ => AspectSummary {
            title: "Valid",
            icon: StudioIconName::StepComplete,
            tone: AspectTone::Good,
            highlight: None,
        },
    }
}

fn edit_state_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Error) => AspectSummary {
            title: "Write failed",
            icon: StudioIconName::StatusError,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Error),
        },
        Some(UiSlotAffordance::Edited) => AspectSummary {
            title: "Edited",
            icon: StudioIconName::Edited,
            tone: AspectTone::Warning,
            highlight: Some(UiSlotAffordance::Edited),
        },
        Some(UiSlotAffordance::Saving) => AspectSummary {
            title: "Saving",
            icon: StudioIconName::StatusRunning,
            tone: AspectTone::Working,
            highlight: Some(UiSlotAffordance::Saving),
        },
        _ => AspectSummary {
            title: "No changes",
            icon: StudioIconName::StepComplete,
            tone: AspectTone::Good,
            highlight: None,
        },
    }
}

fn binding_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Bound) => AspectSummary {
            title: "Bound value",
            icon: StudioIconName::BoundValue,
            tone: AspectTone::Accent,
            highlight: Some(UiSlotAffordance::Bound),
        },
        _ if first_row_label_is(aspect, "Unbound") => AspectSummary {
            title: "Unbound",
            icon: StudioIconName::BoundValue,
            tone: AspectTone::Quiet,
            highlight: None,
        },
        _ => AspectSummary {
            title: "Direct value",
            icon: StudioIconName::AssignedValue,
            tone: AspectTone::Quiet,
            highlight: None,
        },
    }
}

fn first_row_label_is(aspect: &UiSlotAspect, label: &str) -> bool {
    aspect
        .rows
        .first()
        .is_some_and(|row| row.label.eq_ignore_ascii_case(label))
}

fn aspect_detail_lines(aspect: &UiSlotAspect) -> Vec<String> {
    if aspect.kind == UiSlotAspectKind::TypeInfo {
        return Vec::new();
    }

    aspect
        .rows
        .iter()
        .flat_map(|row| {
            let mut lines = Vec::new();
            if !row.value.is_empty() {
                lines.push(row.value.clone());
            }
            if let Some(detail) = row.detail.as_ref().filter(|detail| !detail.is_empty()) {
                lines.push(detail.clone());
            }
            lines
        })
        .collect()
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotInfoRow(row: UiSlotAspectRow) -> Element {
    let is_monospace = matches!(row.label.as_str(), "Name" | "Path");
    let value_class = if is_monospace {
        "tw:font-mono tw:text-xs tw:leading-snug tw:text-muted-foreground tw:break-words"
    } else {
        "tw:text-xs tw:leading-snug tw:text-muted-foreground tw:break-words"
    };

    rsx! {
        if !row.value.is_empty() {
            span { class: value_class, "{row.value}" }
        }
        if let Some(detail) = row.detail {
            if !detail.is_empty() {
                span { class: "tw:text-xs tw:leading-snug tw:text-subtle-foreground tw:break-words", "{detail}" }
            }
        }
    }
}

fn aspect_section_class(highlight: Option<UiSlotAffordance>) -> &'static str {
    match highlight {
        Some(UiSlotAffordance::Error | UiSlotAffordance::Invalid) => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5"
        }
        Some(UiSlotAffordance::Edited) => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5"
        }
        Some(UiSlotAffordance::Saving) => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5"
        }
        Some(UiSlotAffordance::Bound) => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg)_0%,transparent_72%)] tw:px-3 tw:py-1.5"
        }
        Some(UiSlotAffordance::Info) | None => {
            "tw:grid tw:gap-0.5 tw:border-t tw:border-border-muted tw:px-3 tw:py-1.5"
        }
    }
}

fn aspect_icon_class(tone: AspectTone) -> &'static str {
    match tone {
        AspectTone::Error => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-status-error-foreground"
        }
        AspectTone::Warning => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-status-warning-foreground"
        }
        AspectTone::Working => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-status-working-foreground"
        }
        AspectTone::Good => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-status-good-foreground"
        }
        AspectTone::Accent => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-accent"
        }
        AspectTone::Quiet => {
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-heading"
        }
    }
}

fn aspect_heading_class(tone: AspectTone) -> &'static str {
    match tone {
        AspectTone::Error => "tw:m-0 tw:text-xs tw:font-bold tw:text-status-error-foreground",
        AspectTone::Warning => "tw:m-0 tw:text-xs tw:font-bold tw:text-status-warning-foreground",
        AspectTone::Working => "tw:m-0 tw:text-xs tw:font-bold tw:text-status-working-foreground",
        AspectTone::Good => "tw:m-0 tw:text-xs tw:font-bold tw:text-status-good-foreground",
        AspectTone::Accent => "tw:m-0 tw:text-xs tw:font-bold tw:text-accent",
        AspectTone::Quiet => "tw:m-0 tw:text-xs tw:font-bold tw:text-heading",
    }
}
