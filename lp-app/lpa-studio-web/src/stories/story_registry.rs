use dioxus::prelude::*;

use crate::stories::story::StoryDescriptor;

pub const DEFAULT_STORY_ID: &str = "studio/simulator-idle";

mod generated {
    include!(concat!(env!("OUT_DIR"), "/story_registry.generated.rs"));
}

pub fn all_stories() -> Vec<StoryDescriptor> {
    generated::all_generated_stories()
}

pub fn story_by_id(id: &str) -> Option<StoryDescriptor> {
    all_stories().into_iter().find(|story| story.id == id)
}

pub fn render_story(id: &str) -> Element {
    generated::render_generated_story(id).unwrap_or_else(|| {
        rsx! {
            section { class: "ux-panel",
                div { class: "ux-panel-heading",
                    h2 { "Story not found" }
                }
                p { class: "ux-panel-copy", "No story is registered for `{id}`." }
            }
        }
    })
}
