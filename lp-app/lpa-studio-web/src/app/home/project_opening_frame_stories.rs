//! Opening-frame story: what a project reload shows before the sim is up.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::home::ProjectOpeningFrame;

#[story]
fn overview() -> Element {
    rsx! {
        section { class: "tw:p-4",
            ProjectOpeningFrame {}
        }
    }
}
