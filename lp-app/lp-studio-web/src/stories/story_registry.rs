use dioxus::prelude::*;

use crate::components::{
    device_panel_stories, inventory_view_stories, log_panel_stories, project_panel_stories,
    status_bar_stories,
};
use crate::stories::story::StoryDescriptor;

pub const DEFAULT_STORY_ID: &str = "status/idle";

pub fn all_stories() -> Vec<StoryDescriptor> {
    let mut stories = Vec::new();
    stories.extend_from_slice(status_bar_stories::STORIES);
    stories.extend_from_slice(device_panel_stories::STORIES);
    stories.extend_from_slice(project_panel_stories::STORIES);
    stories.extend_from_slice(inventory_view_stories::STORIES);
    stories.extend_from_slice(log_panel_stories::STORIES);
    stories
}

pub fn story_by_id(id: &str) -> Option<StoryDescriptor> {
    all_stories().into_iter().find(|story| story.id == id)
}

pub fn render_story(id: &str) -> Element {
    status_bar_stories::render_story(id)
        .or_else(|| device_panel_stories::render_story(id))
        .or_else(|| project_panel_stories::render_story(id))
        .or_else(|| inventory_view_stories::render_story(id))
        .or_else(|| log_panel_stories::render_story(id))
        .unwrap_or_else(|| {
            rsx! {
                section { class: "panel",
                    div { class: "panel-heading",
                        h2 { "Story not found" }
                    }
                    p { "No story is registered for `{id}`." }
                }
            }
        })
}
