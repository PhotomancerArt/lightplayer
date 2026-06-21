use dioxus::prelude::*;

use crate::components::log_panel::LogPanel;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{
    studio_state_error, studio_state_idle, studio_state_log_heavy, studio_state_ready,
};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "logs/empty",
        "LogPanel",
        "Empty",
        "No logs or diagnostics yet.",
    ),
    StoryDescriptor::new(
        "logs/ready",
        "LogPanel",
        "Ready Logs",
        "A short successful runtime log.",
    ),
    StoryDescriptor::new(
        "logs/diagnostic",
        "LogPanel",
        "Diagnostic",
        "A connection diagnostic appears before logs.",
    ),
    StoryDescriptor::new(
        "logs/heavy",
        "LogPanel",
        "Log Heavy",
        "Many log levels and enough entries to exercise truncation.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "logs/empty" => Some(rsx! { LogPanel { state: studio_state_idle() } }),
        "logs/ready" => Some(rsx! { LogPanel { state: studio_state_ready() } }),
        "logs/diagnostic" => Some(rsx! { LogPanel { state: studio_state_error() } }),
        "logs/heavy" => Some(rsx! { LogPanel { state: studio_state_log_heavy() } }),
        _ => None,
    }
}
