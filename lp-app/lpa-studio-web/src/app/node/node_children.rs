use dioxus::prelude::*;
use lpa_studio_core::UiNodeChild;

use crate::app::node::{DirtyMark, NodeSection};
use crate::core::StatusChip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeChildren(items: Vec<UiNodeChild>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2 tw:border-l tw:border-border-muted tw:pl-4",
            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-none tw:text-heading", "Children" }
            div { class: "tw:grid tw:min-w-0 tw:gap-2",
                for child in items {
                    ChildNodeCard { child }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChildNodeCard(child: UiNodeChild) -> Element {
    let class = if child.active {
        "tw:grid tw:min-w-0 tw:gap-3 tw:rounded-md tw:border tw:border-accent-border tw:bg-card tw:p-3"
    } else {
        "tw:grid tw:min-w-0 tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3"
    };

    rsx! {
        article { class,
            header { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start tw:justify-between tw:gap-2",
                div { class: "tw:min-w-0",
                    div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                        h5 { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground tw:break-words", "{child.label}" }
                        DirtyMark { dirty: child.dirty }
                    }
                    p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground tw:break-words", "{child.kind} - {child.detail}" }
                }
                StatusChip { status: child.status.clone() }
            }
            if let Some(summary) = child.summary.as_ref() {
                p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground tw:break-words", "{summary}" }
            }
            if !child.sections.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-3",
                    for section in child.sections {
                        NodeSection { section }
                    }
                }
            }
        }
    }
}
