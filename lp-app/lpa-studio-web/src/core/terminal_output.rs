use dioxus::prelude::*;
use lpa_studio_core::UiTerminalLine;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TerminalOutput(lines: Vec<UiTerminalLine>) -> Element {
    if lines.is_empty() {
        return rsx! {};
    }

    rsx! {
        ol { class: "tw:m-0 tw:grid tw:max-h-60 tw:gap-1 tw:overflow-auto tw:rounded-sm tw:border tw:border-border-subtle tw:bg-terminal tw:p-3 tw:font-mono tw:text-[0.78rem] tw:leading-snug tw:text-muted-foreground",
            for line in lines {
                li { class: "tw:min-w-0 tw:list-none tw:break-words", "{line.text}" }
            }
        }
    }
}
