use dioxus::prelude::*;
use lpa_studio_core::UiProgress;
use lpa_studio_web_story_macros::story;

use crate::core::ProgressBar;

#[story]
pub(crate) fn variants() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-[18px]",
            ProgressBar {
                progress: UiProgress::indeterminate("Opening link session")
                    .with_detail("Waiting for the browser serial provider."),
            }
            ProgressBar {
                progress: UiProgress::determinate("Writing firmware", 42)
                    .with_detail("app image at 0x10000"),
            }
            ProgressBar {
                progress: UiProgress::timeout("Waiting for boot", 5000)
                    .with_detail("Studio will retry when the device responds."),
            }
        }
    }
}
