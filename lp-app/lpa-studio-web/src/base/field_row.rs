use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FieldRow(label: String, value: String, changed: bool, detail: Option<String>) -> Element {
    let class = if changed {
        "tw:grid tw:grid-cols-[minmax(120px,0.35fr)_minmax(0,1fr)] tw:gap-3 tw:rounded-sm tw:border tw:border-accent-border tw:bg-status-good-bg tw:p-3"
    } else {
        "tw:grid tw:grid-cols-[minmax(120px,0.35fr)_minmax(0,1fr)] tw:gap-3 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-3"
    };

    rsx! {
        div { class,
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                span { "{label}" }
                if changed {
                    small { class: "tw:text-xs tw:font-bold tw:uppercase tw:text-accent", "modified" }
                }
            }
            div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:text-right",
                span { "{value}" }
                if let Some(detail) = detail.as_ref() {
                    small { class: "tw:text-xs tw:text-subtle-foreground", "{detail}" }
                }
            }
        }
    }
}
