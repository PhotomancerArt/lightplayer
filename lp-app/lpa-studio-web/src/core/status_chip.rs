use dioxus::prelude::*;
use lpa_studio_core::UiStatus;
use lpa_studio_core::core::status::UiStatusKind;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StatusChip(status: UiStatus) -> Element {
    rsx! {
        span { class: "{status_class(status.kind)}", "{status.label}" }
    }
}

pub fn status_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-neutral-foreground tw:break-words"
        }
        UiStatusKind::Working => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-working-foreground tw:break-words"
        }
        UiStatusKind::Good => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-good-foreground tw:break-words"
        }
        UiStatusKind::Warning => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-warning-foreground tw:break-words"
        }
        UiStatusKind::Attention => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-attention-foreground tw:break-words"
        }
        UiStatusKind::Error => {
            "tw:inline-flex tw:min-h-6 tw:max-w-full tw:flex-shrink tw:items-center tw:rounded-pill tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:text-xs tw:font-bold tw:leading-none tw:text-status-error-foreground tw:break-words"
        }
    }
}
