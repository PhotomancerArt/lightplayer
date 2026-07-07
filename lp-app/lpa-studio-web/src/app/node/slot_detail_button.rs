//! Shared slot detail button and row treatment for node slot-like surfaces.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
};

use crate::app::node::slot_edit_actions::slot_revert_action;
use crate::app::node::{
    SlotShapeDisplay, SlotShapeDisplayMode, SlotUnitDisplay, SlotUnitDisplayMode,
    legacy_shape_from_parts,
};
use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName,
    detail_popover_section_class,
};

/// Revert/reset affordance rendered INSIDE the slot detail popup's edited
/// (edit-state) section for a touched editable slot — beside the state and
/// old-value rows it acts on, like the save panel's per-entry revert rows
/// (the row's inline icon stays the quick path).
#[derive(Clone, PartialEq)]
pub struct SlotDetailRevert {
    /// Button label: "Revert" for unsaved persisted edits, "Reset" for live
    /// (transient) controls.
    pub label: &'static str,
    /// Tooltip explaining what dispatching the revert discards.
    pub title: &'static str,
    /// Slot address the revert op targets.
    pub address: ProjectSlotAddress,
    /// Shared action conduit.
    pub on_action: EventHandler<UiAction>,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotDetailButton(
    label: String,
    aspects: Vec<UiSlotAspect>,
    #[props(default = false)] initially_open: bool,
    #[props(default = None)] revert: Option<SlotDetailRevert>,
) -> Element {
    let affordance = primary_affordance(&aspects);
    let style = slot_affordance_style(affordance);
    let menu_label = format!("{label} details");

    rsx! {
        span { class: "tw:inline-flex tw:w-8 tw:justify-end",
            DetailPopover {
                icon: style.icon,
                label: menu_label.clone(),
                title: menu_label,
                tone: style.tone,
                placement: PopoverPlacement::BottomEnd,
                active: style.active,
                initially_open,
                for aspect in aspects {
                    SlotDetailSection {
                        // The revert lives INSIDE the edited (edit-state)
                        // section it acts on — no floating popup footer.
                        revert: (aspect.kind == UiSlotAspectKind::EditState)
                            .then(|| revert.clone())
                            .flatten(),
                        aspect,
                    }
                }
            }
        }
    }
}

/// The in-section revert/reset button: the shared revert icon plus the verb,
/// right-aligned under the edited section's rows (mirroring the save panel's
/// per-entry revert buttons).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotDetailRevertButton(revert: SlotDetailRevert) -> Element {
    let SlotDetailRevert {
        label,
        title,
        address,
        on_action,
    } = revert;

    rsx! {
        div { class: "tw:flex tw:justify-end tw:pt-1",
            button {
                class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-border-strong tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-[0.68rem] tw:font-bold tw:text-muted-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground",
                r#type: "button",
                title,
                onclick: move |event| {
                    event.stop_propagation();
                    on_action.call(slot_revert_action(address.clone()));
                },
                StudioIcon {
                    name: StudioIconName::Revert,
                    size: 12,
                }
                span { "{label}" }
            }
        }
    }
}

pub(crate) fn slot_row_class(affordance: UiSlotAffordance, index: usize) -> &'static str {
    match affordance {
        UiSlotAffordance::Error | UiSlotAffordance::Invalid => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-error-bg)_0%,var(--studio-status-error-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Edited => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-warning-bg)_0%,var(--studio-status-warning-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Saving => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-working-bg)_0%,var(--studio-status-working-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Bound => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-good-bg)_0%,var(--studio-status-good-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Info if index % 2 == 0 => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-color-surface-muted)_0%,var(--studio-color-surface-muted)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
        }
        UiSlotAffordance::Info => {
            "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-color-surface-subtle)_0%,var(--studio-color-surface-subtle)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
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
fn SlotDetailSection(
    aspect: UiSlotAspect,
    #[props(default = None)] revert: Option<SlotDetailRevert>,
) -> Element {
    let summary = aspect_summary(&aspect);
    let section_class = detail_popover_section_class(aspect_section_tint(summary.highlight));
    let heading_class = aspect_heading_class(summary.tone);
    let icon_class = aspect_icon_class(summary.tone);
    let details = aspect_detail_rows(&aspect);
    let info_rows = if aspect.kind == UiSlotAspectKind::TypeInfo {
        type_info_detail_rows(&aspect)
    } else {
        Vec::new()
    };
    let title = summary.title.clone();
    let code = summary.code.clone();
    let title_is_code = summary.title_is_code;

    rsx! {
        section { class: section_class,
            header { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:leading-none",
                if aspect.kind != UiSlotAspectKind::TypeInfo {
                    span { class: icon_class,
                        StudioIcon {
                            name: summary.icon,
                            size: 12,
                        }
                    }
                }
                if title_is_code {
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:font-bold tw:text-heading", "{title}" }
                } else {
                    h3 { class: heading_class, "{title}" }
                }
                if let Some(code) = code {
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-muted-foreground", "{code}" }
                }
            }
            if aspect.kind == UiSlotAspectKind::TypeInfo {
                div { class: "tw:grid tw:gap-0.5 tw:pt-0.5",
                    for row in info_rows {
                        SlotInfoRow { row }
                    }
                }
            } else if !details.is_empty() {
                div { class: "tw:grid tw:gap-0.5 tw:pl-[18px] tw:pt-0.5",
                    for row in details {
                        SlotDetailRow { row }
                    }
                }
            }
            if let Some(revert) = revert {
                SlotDetailRevertButton { revert }
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

#[derive(Clone)]
struct AspectSummary {
    title: String,
    code: Option<String>,
    title_is_code: bool,
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
            icon: StudioIconName::InfoBare,
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

fn aspect_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.kind {
        UiSlotAspectKind::Optionality => optionality_summary(aspect),
        UiSlotAspectKind::Validation => validation_summary(aspect),
        UiSlotAspectKind::EditState => edit_state_summary(aspect),
        UiSlotAspectKind::Binding => binding_summary(aspect),
        UiSlotAspectKind::TypeInfo => AspectSummary {
            title: type_info_title(aspect),
            code: None,
            title_is_code: true,
            icon: StudioIconName::InfoBare,
            tone: AspectTone::Quiet,
            highlight: None,
        },
    }
}

fn optionality_summary(aspect: &UiSlotAspect) -> AspectSummary {
    if first_row_label_is(aspect, "Enabled") {
        AspectSummary {
            title: "Enabled".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::AssignedValue,
            tone: AspectTone::Good,
            highlight: None,
        }
    } else {
        AspectSummary {
            title: "Disabled".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::UnboundValue,
            tone: AspectTone::Quiet,
            highlight: None,
        }
    }
}

fn validation_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Error) => AspectSummary {
            title: "Error".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StatusError,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Error),
        },
        Some(UiSlotAffordance::Invalid) => AspectSummary {
            title: "Invalid".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StepAttention,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Invalid),
        },
        _ => AspectSummary {
            title: "Valid".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StepComplete,
            tone: AspectTone::Good,
            highlight: None,
        },
    }
}

fn edit_state_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Error) => AspectSummary {
            title: "Write failed".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StatusError,
            tone: AspectTone::Error,
            highlight: Some(UiSlotAffordance::Error),
        },
        Some(UiSlotAffordance::Edited) => AspectSummary {
            title: "Edited".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::Edited,
            tone: AspectTone::Warning,
            highlight: Some(UiSlotAffordance::Edited),
        },
        Some(UiSlotAffordance::Saving) => AspectSummary {
            title: "Saving".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StatusRunning,
            tone: AspectTone::Working,
            highlight: Some(UiSlotAffordance::Saving),
        },
        _ => AspectSummary {
            title: "No changes".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::StepComplete,
            tone: AspectTone::Good,
            highlight: None,
        },
    }
}

fn binding_summary(aspect: &UiSlotAspect) -> AspectSummary {
    match aspect.affordance {
        Some(UiSlotAffordance::Bound) => AspectSummary {
            title: binding_title(aspect),
            code: first_row_value(aspect),
            title_is_code: false,
            icon: StudioIconName::BoundValue,
            tone: AspectTone::Accent,
            highlight: Some(UiSlotAffordance::Bound),
        },
        _ if first_row_label_is(aspect, "Unbound") => AspectSummary {
            title: "Unbound".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::UnboundValue,
            tone: AspectTone::Quiet,
            highlight: None,
        },
        _ => AspectSummary {
            title: "Unbound".to_string(),
            code: None,
            title_is_code: false,
            icon: StudioIconName::AssignedValue,
            tone: AspectTone::Quiet,
            highlight: None,
        },
    }
}

fn binding_title(aspect: &UiSlotAspect) -> String {
    match aspect.rows.first().map(|row| row.label.as_str()) {
        Some(label) if label.eq_ignore_ascii_case("Published") => "Published as".to_string(),
        Some(label) if label.eq_ignore_ascii_case("Bound to") => "Bound to".to_string(),
        Some(label) if label.eq_ignore_ascii_case("Consumed by") => "Consumed by".to_string(),
        _ => "Bound from".to_string(),
    }
}

fn first_row_label_is(aspect: &UiSlotAspect, label: &str) -> bool {
    aspect
        .rows
        .first()
        .is_some_and(|row| row.label.eq_ignore_ascii_case(label))
}

fn aspect_detail_rows(aspect: &UiSlotAspect) -> Vec<UiSlotAspectRow> {
    if aspect.kind == UiSlotAspectKind::TypeInfo {
        return Vec::new();
    }
    let value_is_header_code = aspect.kind == UiSlotAspectKind::Binding
        && aspect.affordance == Some(UiSlotAffordance::Bound);

    aspect
        .rows
        .iter()
        .enumerate()
        .filter(|(index, row)| {
            !(value_is_header_code && *index == 0)
                && (!row.value.is_empty()
                    || row.detail.as_ref().is_some_and(|detail| !detail.is_empty()))
        })
        .map(|(_, row)| row.clone())
        .collect()
}

fn type_info_title(aspect: &UiSlotAspect) -> String {
    aspect
        .rows
        .iter()
        .find(|row| row.label.eq_ignore_ascii_case("Path") && !row.value.is_empty())
        .or_else(|| {
            aspect
                .rows
                .iter()
                .find(|row| row.label.eq_ignore_ascii_case("Name") && !row.value.is_empty())
        })
        .map(|row| row.value.clone())
        .unwrap_or_else(|| aspect.title.clone())
}

fn type_info_detail_rows(aspect: &UiSlotAspect) -> Vec<UiSlotAspectRow> {
    aspect
        .rows
        .iter()
        .filter(|row| {
            !row.label.eq_ignore_ascii_case("Path") && !row.label.eq_ignore_ascii_case("Name")
        })
        .cloned()
        .collect()
}

fn first_row_value(aspect: &UiSlotAspect) -> Option<String> {
    aspect
        .rows
        .first()
        .map(|row| row.value.clone())
        .filter(|value| !value.is_empty())
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotInfoRow(row: UiSlotAspectRow) -> Element {
    let shape = row.shape.clone().or_else(|| {
        row.label
            .eq_ignore_ascii_case("Shape")
            .then(|| legacy_shape_from_parts(&row.value, row.detail.as_deref()))
    });
    let unit = row.unit.clone();

    rsx! {
        if let Some(shape) = shape {
            p { class: "tw:m-0 tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
                SlotShapeDisplay {
                    shape,
                    mode: SlotShapeDisplayMode::CompactFriendly,
                }
            }
        } else if let Some(unit) = unit {
            p { class: "tw:m-0 tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
                span { class: "tw:font-bold tw:text-subtle-foreground", "{row.label}:" }
                SlotUnitDisplay {
                    unit,
                    mode: SlotUnitDisplayMode::Long,
                }
            }
        } else if !row.value.is_empty() {
            p { class: "tw:m-0 tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
                if !row.label.eq_ignore_ascii_case("Shape") {
                    span { class: "tw:font-bold tw:text-subtle-foreground", "{row.label}:" }
                }
                span { class: "tw:text-muted-foreground tw:break-words", "{row.value}" }
                if let Some(detail) = row.detail {
                    if !detail.is_empty() {
                        span { class: "tw:text-subtle-foreground tw:break-words", "{detail}" }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotDetailRow(row: UiSlotAspectRow) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
            if !row.label.is_empty() {
                span { class: "tw:font-bold tw:text-subtle-foreground", "{row.label}:" }
            }
            if !row.value.is_empty() {
                span { class: "tw:text-muted-foreground tw:break-words", "{row.value}" }
            }
            if let Some(detail) = row.detail {
                if !detail.is_empty() {
                    span { class: "tw:text-subtle-foreground tw:break-words", "{detail}" }
                }
            }
        }
    }
}

/// Map a slot aspect's highlight affordance onto the shared detail-card
/// section tints.
fn aspect_section_tint(highlight: Option<UiSlotAffordance>) -> DetailSectionTint {
    match highlight {
        Some(UiSlotAffordance::Error | UiSlotAffordance::Invalid) => DetailSectionTint::Error,
        Some(UiSlotAffordance::Edited) => DetailSectionTint::Warning,
        Some(UiSlotAffordance::Saving) => DetailSectionTint::Working,
        Some(UiSlotAffordance::Bound) => DetailSectionTint::Good,
        Some(UiSlotAffordance::Info) | None => DetailSectionTint::None,
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
