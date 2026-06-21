use dioxus::prelude::*;

use crate::components::project_panel::ProjectPanel;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{
    studio_state_idle, studio_state_long_content, studio_state_ready,
};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "project/not-loaded",
        "ProjectPanel",
        "Not Loaded",
        "No project session exists yet.",
    ),
    StoryDescriptor::new(
        "project/ready",
        "ProjectPanel",
        "Ready",
        "The demo project is loaded.",
    ),
    StoryDescriptor::new(
        "project/long-content",
        "ProjectPanel",
        "Long Content",
        "Long project and selection labels should wrap.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "project/not-loaded" => Some(rsx! { ProjectPanel { state: studio_state_idle() } }),
        "project/ready" => Some(rsx! { ProjectPanel { state: studio_state_ready() } }),
        "project/long-content" => {
            Some(rsx! { ProjectPanel { state: studio_state_long_content() } })
        }
        _ => None,
    }
}
