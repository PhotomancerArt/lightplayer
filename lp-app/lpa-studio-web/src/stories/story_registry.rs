use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;

pub const DEFAULT_STORY_ID: &str = "studio/layout/studio-shell/simulator-idle";

mod generated {
    include!(concat!(env!("OUT_DIR"), "/story_registry.generated.rs"));
}

/// Return every generated story descriptor.
///
/// The source of truth is the set of `#[story]` functions discovered by
/// `lpa-studio-web/build.rs`; this module intentionally contains no hand-written
/// story list.
pub fn all_stories() -> Vec<StoryDescriptor> {
    generated::all_generated_stories()
}

pub fn story_by_id(id: &str) -> Option<StoryDescriptor> {
    all_stories().into_iter().find(|story| story.id == id)
}

pub fn render_story(id: &str) -> Element {
    generated::render_generated_story(id).unwrap_or_else(|| {
        rsx! {
            section { class: "tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-[18px]",
                div { class: "tw:mb-3 tw:flex tw:flex-wrap tw:items-center tw:justify-between tw:gap-3",
                    h2 { class: "tw:m-0 tw:text-base tw:font-bold tw:text-strong-foreground", "Story not found" }
                }
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-muted-foreground", "No story is registered for `{id}`." }
            }
        }
    })
}
