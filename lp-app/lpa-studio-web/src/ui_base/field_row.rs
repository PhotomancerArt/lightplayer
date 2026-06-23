use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FieldRow(label: String, value: String, changed: bool, detail: Option<String>) -> Element {
    let class = if changed {
        "ux-field-row ux-field-row-changed"
    } else {
        "ux-field-row"
    };

    rsx! {
        div { class,
            div { class: "ux-field-label",
                span { "{label}" }
                if changed {
                    small { "modified" }
                }
            }
            div { class: "ux-field-value",
                span { "{value}" }
                if let Some(detail) = detail.as_ref() {
                    small { "{detail}" }
                }
            }
        }
    }
}
