use dioxus::prelude::*;
use lpa_studio_core::UiTerminalLine;
use lpa_studio_web_story_macros::story;

use crate::core::TerminalOutput;
use crate::core::story_fixtures::story_terminal_lines;

#[story]
pub(crate) fn short_output() -> Element {
    rsx! {
        TerminalOutput {
            lines: story_terminal_lines(),
        }
    }
}

#[story]
pub(crate) fn wrapped_output() -> Element {
    rsx! {
        TerminalOutput {
            lines: vec![
                UiTerminalLine::new("[fw-esp32] ESP-ROM:esp32c6-20220919"),
                UiTerminalLine::new("[lp-server] project shape response contained node /demo/shaders/orbit with 6 slots and 2 runtime bindings"),
                UiTerminalLine::new("[studio] overlay has 2 pending changes"),
            ],
        }
    }
}
