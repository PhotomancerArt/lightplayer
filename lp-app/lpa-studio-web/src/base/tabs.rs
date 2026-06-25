use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Tabs(tabs: Vec<TabItem>, initial: usize) -> Element {
    let initial = initial.min(tabs.len().saturating_sub(1));
    let mut active = use_signal(|| initial);
    let active_index = active().min(tabs.len().saturating_sub(1));
    let active_tab = tabs.get(active_index).cloned();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            div { class: "tw:flex tw:flex-wrap tw:gap-2", role: "tablist",
                for (index, tab) in tabs.clone().into_iter().enumerate() {
                    button {
                        class: tab_class(index == active_index),
                        r#type: "button",
                        role: "tab",
                        aria_selected: "{index == active_index}",
                        onclick: move |_| active.set(index),
                        "{tab.label}"
                    }
                }
            }
            if let Some(tab) = active_tab {
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-3", role: "tabpanel",
                    h3 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "{tab.title}" }
                    p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-muted-foreground", "{tab.body}" }
                }
            }
        }
    }
}

fn tab_class(active: bool) -> &'static str {
    if active {
        "tw:min-h-8 tw:rounded-sm tw:border tw:border-accent-border tw:bg-status-good-bg tw:px-3 tw:text-sm tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:min-h-8 tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-3 tw:text-sm tw:text-muted-foreground tw:hover:bg-card-muted"
    }
}

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
