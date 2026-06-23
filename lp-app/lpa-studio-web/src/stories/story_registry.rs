use dioxus::prelude::*;

use crate::stories::node_ui_spike_stories;
use crate::stories::story::StoryDescriptor;
use crate::stories::studio_ux_stories;

pub const DEFAULT_STORY_ID: &str = "studio/simulator-idle";

pub fn all_stories() -> Vec<StoryDescriptor> {
    let mut stories = studio_ux_stories::STORIES.to_vec();
    stories.extend_from_slice(node_ui_spike_stories::STORIES);
    stories
}

pub fn story_by_id(id: &str) -> Option<StoryDescriptor> {
    all_stories().into_iter().find(|story| story.id == id)
}

pub fn render_story(id: &str) -> Element {
    studio_ux_stories::render_story(id)
        .or_else(|| node_ui_spike_stories::render_story(id))
        .unwrap_or_else(|| {
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
