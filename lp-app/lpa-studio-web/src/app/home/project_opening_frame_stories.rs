//! Opening-frame story: what a project reload shows before the sim is up.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::home::ProjectOpeningFrame;

// Named `default`, not `overview`: `<component>/overview` is reserved for the
// generated composite page, which shadowed this story outright — the id
// resolved to the composite, so the authored page was unreachable and its
// baseline was a composite capture. See `OVERVIEW_ID_SUFFIX` in
// `stories/story_book.rs`.
#[story]
fn default() -> Element {
    rsx! {
        section { class: "tw:p-4",
            ProjectOpeningFrame {}
        }
    }
}
