use dioxus::prelude::*;
use lpa_studio_core::UiNodeHeader;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeHeader(header: UiNodeHeader) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:px-4",
            div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                h3 { class: "tw:m-0 tw:flex tw:min-w-0 tw:items-baseline tw:gap-2 tw:text-[1.04rem] tw:font-bold tw:leading-tight tw:text-strong-foreground",
                    span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap", "{header.title}" }
                    if let Some(summary) = header.summary.as_ref() {
                        small { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-xs tw:font-bold tw:text-subtle-foreground", "{summary}" }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:gap-x-2 tw:gap-y-0.5 tw:text-xs tw:text-subtle-foreground",
                    span { class: "tw:font-bold", "{header.kind}" }
                    if let Some(source) = header.source.as_ref() {
                        span { class: "tw:min-w-0 tw:break-words", "{source}" }
                    }
                    span { class: "tw:min-w-0 tw:font-mono tw:break-words", "{header.path}" }
                }
            }
        }
    }
}
