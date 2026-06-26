use dioxus::prelude::*;
use lpa_studio_core::UiNodeHeader;
use lpa_studio_core::core::status::UiStatusKind;

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeHeader(header: UiNodeHeader) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:px-3",
            NodeStatusMenu { header: header.clone() }
            h3 { class: "tw:m-0 tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-[1.04rem] tw:font-bold tw:leading-tight tw:text-strong-foreground",
                "{header.title}"
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeStatusMenu(header: UiNodeHeader) -> Element {
    let icon = status_icon(header.status.kind);
    let label = format!("{} status details", header.title);

    rsx! {
        IconPopoverButton {
            class: node_status_button_class(header.status.kind).to_string(),
            open_class: node_status_button_open_class(header.status.kind).to_string(),
            icon,
            icon_size: 13,
            label,
            title: format!("{} status details", header.title),
            popup_class: node_status_popup_class(header.status.kind).to_string(),
            chrome_class: node_status_chrome_class(header.status.kind).to_string(),
            placement: PopoverPlacement::BottomStart,
            div { class: "tw:grid tw:min-w-0 tw:gap-3 tw:p-3",
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:justify-between tw:gap-4",
                    div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{header.title}" }
                        span { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground", "{header.kind}" }
                    }
                    span { class: node_status_label_class(header.status.kind), "{header.status.label}" }
                }
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-2 tw:text-xs",
                    if let Some(summary) = header.summary.as_ref() {
                        NodeStatusDetailRow {
                            label: "summary",
                            value: summary.clone(),
                        }
                    }
                    if let Some(source) = header.source.as_ref() {
                        NodeStatusDetailRow {
                            label: "source",
                            value: source.clone(),
                        }
                    }
                    NodeStatusDetailRow {
                        label: "path",
                        value: header.path.clone(),
                    }
                }
                if let Some(detail) = header.detail.as_ref() {
                    p { class: "tw:m-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2 tw:text-xs tw:leading-normal tw:text-muted-foreground tw:break-words", "{detail}" }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NodeStatusDetailRow(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[72px_minmax(0,1fr)] tw:gap-2",
            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words", "{value}" }
        }
    }
}

fn status_icon(kind: UiStatusKind) -> StudioIconName {
    match kind {
        UiStatusKind::Neutral => StudioIconName::StatusIdle,
        UiStatusKind::Working | UiStatusKind::Good => StudioIconName::StatusRunning,
        UiStatusKind::Warning => StudioIconName::StepAttention,
        UiStatusKind::Error => StudioIconName::StatusError,
    }
}

fn node_status_button_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:p-0 tw:text-status-neutral-foreground"
        }
        UiStatusKind::Working => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground"
        }
        UiStatusKind::Good => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-good-border tw:bg-status-good-bg tw:p-0 tw:text-status-good-foreground"
        }
        UiStatusKind::Warning => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground"
        }
        UiStatusKind::Error => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground"
        }
    }
}

fn node_status_button_open_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-neutral-border tw:bg-card-raised tw:p-0 tw:text-status-neutral-foreground"
        }
        UiStatusKind::Working => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-working-border tw:bg-card-raised tw:p-0 tw:text-status-working-foreground"
        }
        UiStatusKind::Good => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-good-border tw:bg-card-raised tw:p-0 tw:text-status-good-foreground"
        }
        UiStatusKind::Warning => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-warning-border tw:bg-card-raised tw:p-0 tw:text-status-warning-foreground"
        }
        UiStatusKind::Error => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-status-error-border tw:bg-card-raised tw:p-0 tw:text-status-error-foreground"
        }
    }
}

fn node_status_popup_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-neutral-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg"
        }
        UiStatusKind::Working => {
            "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg"
        }
        UiStatusKind::Good => {
            "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-good-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-good-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg"
        }
        UiStatusKind::Warning => {
            "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-warning-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg"
        }
        UiStatusKind::Error => {
            "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:overflow-hidden tw:rounded-md tw:border tw:border-status-error-border tw:bg-card tw:bg-[linear-gradient(90deg,var(--studio-status-error-bg),transparent_74%)] tw:text-sm tw:text-muted-foreground tw:shadow-lg"
        }
    }
}

fn node_status_chrome_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => "ux-popover-chrome-neutral",
        UiStatusKind::Working => "ux-popover-chrome-working",
        UiStatusKind::Good => "ux-popover-chrome-good",
        UiStatusKind::Warning => "ux-popover-chrome-warning",
        UiStatusKind::Error => "ux-popover-chrome-error",
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
