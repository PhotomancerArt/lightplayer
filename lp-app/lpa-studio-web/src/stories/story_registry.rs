use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;

/// The story the book opens on, and the fallback for an unknown route.
///
/// Must name a story that actually exists — see the guard in
/// `story_book::StoryBook`, which falls back to the first registered
/// story rather than blanking the page if this ever goes stale. It did:
/// the previous default (`studio/layout/studio-shell/simulator-idle`)
/// retired with the step-stack device pane, and the storybook rendered
/// an empty body, so capture discovered ZERO stories.
pub const DEFAULT_STORY_ID: &str = "studio/home/home-gallery/populated";

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

pub fn generated_at_utc() -> &'static str {
    generated::GENERATED_AT_UTC
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
