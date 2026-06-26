use dioxus::prelude::*;
use lpa_studio_core::UiStatus;

use crate::core::StatusChip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaneFrame(
    title: String,
    primary: bool,
    status: Option<UiStatus>,
    children: Element,
) -> Element {
    let panel_class = if primary {
        "tw:rounded-md tw:border tw:border-panel-primary-border tw:bg-panel-primary tw:p-[18px]"
    } else {
        "tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-[18px]"
    };

    rsx! {
        section { class: "{panel_class}",
            div { class: "tw:mb-3 tw:flex tw:flex-wrap tw:items-center tw:justify-between tw:gap-3",
                p { class: "tw:m-0 tw:text-xs tw:font-extrabold tw:uppercase tw:leading-none tw:text-heading", "{title}" }
                if let Some(status) = status {
                    StatusChip { status }
                }
            }
            {children}
        }
    }
}
