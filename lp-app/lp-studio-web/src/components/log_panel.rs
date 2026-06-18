use dioxus::prelude::*;
use lp_studio_core::StudioState;

#[component]
pub fn LogPanel(state: StudioState) -> Element {
    rsx! {
        section { class: "panel log-panel",
            div { class: "panel-heading",
                h2 { "Logs" }
                span { class: "mini-count", "{state.logs.len()}" }
            }
            ul { class: "log-list",
                for diagnostic in state.diagnostics.iter().rev().take(4) {
                    li { class: "diagnostic-line", "{diagnostic.severity:?}: {diagnostic.message}" }
                }
                for entry in state.logs.iter().rev().take(10) {
                    li { "{entry.level:?} {entry.target}: {entry.message}" }
                }
                if state.logs.is_empty() && state.diagnostics.is_empty() {
                    li { "No runtime logs yet." }
                }
            }
        }
    }
}
