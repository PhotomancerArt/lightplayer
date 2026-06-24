use dioxus::prelude::*;
use lpa_studio_core::{UiStatus, UiStatusKind};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StatusChip(status: UiStatus) -> Element {
    rsx! {
        span { class: "{status_class(status.kind)}", "{status.label}" }
    }
}

pub fn status_class(kind: UiStatusKind) -> &'static str {
    match kind {
        UiStatusKind::Neutral => "ux-status ux-status-neutral",
        UiStatusKind::Working => "ux-status ux-status-working",
        UiStatusKind::Good => "ux-status ux-status-good",
        UiStatusKind::Warning => "ux-status ux-status-warning",
        UiStatusKind::Error => "ux-status ux-status-error",
    }
}
