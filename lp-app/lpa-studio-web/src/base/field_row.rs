use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FieldRow(label: String, value: String, changed: bool, detail: Option<String>) -> Element {
    let class = if changed {
        // "Modified" is an unsaved edit, so the row wears the amber warning
        // family — the app-wide unsaved-edit convention (accent reckoning
        // swept the old accent-on-good mix, D1 2026-08-30).
        "tw:grid tw:grid-cols-[minmax(120px,0.35fr)_minmax(0,1fr)] tw:gap-3 tw:rounded-sm tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-3"
    } else {
        "tw:grid tw:grid-cols-[minmax(120px,0.35fr)_minmax(0,1fr)] tw:gap-3 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-3"
    };

    rsx! {
        div { class,
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                span { "{label}" }
                if changed {
                    small { class: "tw:text-xs tw:font-bold tw:uppercase tw:text-status-warning-foreground", "modified" }
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
