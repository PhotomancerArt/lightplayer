use dioxus::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabItem {
    pub label: String,
    pub title: String,
    pub body: String,
}

impl TabItem {
    pub fn new(
        label: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            title: title.into(),
            body: body.into(),
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Tabs(tabs: Vec<TabItem>, initial: usize) -> Element {
    let initial = initial.min(tabs.len().saturating_sub(1));
    let mut active = use_signal(|| initial);
    let active_index = active().min(tabs.len().saturating_sub(1));
    let active_tab = tabs.get(active_index).cloned();

    rsx! {
        div { class: "ux-tabs",
            div { class: "ux-tab-list", role: "tablist",
                for (index, tab) in tabs.clone().into_iter().enumerate() {
                    button {
                        class: if index == active_index { "ux-tab ux-tab-active" } else { "ux-tab" },
                        r#type: "button",
                        role: "tab",
                        aria_selected: "{index == active_index}",
                        onclick: move |_| active.set(index),
                        "{tab.label}"
                    }
                }
            }
            if let Some(tab) = active_tab {
                div { class: "ux-tab-panel", role: "tabpanel",
                    h3 { "{tab.title}" }
                    p { "{tab.body}" }
                }
            }
        }
    }
}
