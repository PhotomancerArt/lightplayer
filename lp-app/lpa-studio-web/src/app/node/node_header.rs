use dioxus::prelude::*;
use lpa_studio_core::UiNodeHeader;

use crate::core::StatusChip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeHeader(header: UiNodeHeader) -> Element {
    rsx! {
        header { class: "tw:grid tw:grid-cols-[minmax(0,1fr)_auto] tw:items-start tw:gap-3 tw:border-b tw:border-border-muted tw:pb-3",
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-2 tw:gap-y-1",
                    h3 { class: "tw:m-0 tw:text-base tw:font-bold tw:leading-tight tw:text-strong-foreground tw:break-words", "{header.title}" }
                    span { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:text-subtle-foreground", "{header.kind}" }
                }
                p { class: "tw:m-0 tw:font-mono tw:text-xs tw:leading-normal tw:text-subtle-foreground tw:break-words", "{header.path}" }
                if let Some(source) = header.source.as_ref() {
                    p { class: "tw:m-0 tw:text-xs tw:leading-normal tw:text-muted-foreground tw:break-words", "{source}" }
                }
            }
            div { class: "tw:flex tw:flex-col tw:items-end tw:gap-1",
                StatusChip { status: header.status.clone() }
                if let Some(summary) = header.summary.as_ref() {
                    span { class: "tw:text-xs tw:text-subtle-foreground", "{summary}" }
                }
            }
            if let Some(detail) = header.detail.as_ref() {
                p { class: "tw:col-span-2 tw:m-0 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2 tw:text-sm tw:leading-normal tw:text-muted-foreground tw:break-words", "{detail}" }
            }
        }
    }
}
