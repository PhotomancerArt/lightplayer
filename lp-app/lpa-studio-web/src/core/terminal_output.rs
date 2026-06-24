use dioxus::prelude::*;
use lpa_studio_core::UiTerminalLine;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TerminalOutput(lines: Vec<UiTerminalLine>) -> Element {
    if lines.is_empty() {
        return rsx! {};
    }

    rsx! {
        ol { class: "ux-terminal",
            for line in lines {
                li { "{line.text}" }
            }
        }
    }
}
