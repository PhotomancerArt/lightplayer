use dioxus::prelude::*;

use crate::components::status_bar::StatusBar;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{
    studio_state_connected, studio_state_error, studio_state_idle, studio_state_ready,
};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "status/idle",
        "StatusBar",
        "Idle",
        "No runtime is connected.",
    ),
    StoryDescriptor::new(
        "status/starting",
        "StatusBar",
        "Starting",
        "The browser worker is starting.",
    ),
    StoryDescriptor::new(
        "status/ready",
        "StatusBar",
        "Ready",
        "A demo project is loaded with heartbeat data.",
    ),
    StoryDescriptor::new(
        "status/error",
        "StatusBar",
        "Error",
        "Startup failure shown in the status bar.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "status/idle" => {
            Some(rsx! { StatusBar { state: studio_state_idle(), running: false, error: None } })
        }
        "status/starting" => {
            Some(rsx! { StatusBar { state: studio_state_connected(), running: true, error: None } })
        }
        "status/ready" => {
            Some(rsx! { StatusBar { state: studio_state_ready(), running: false, error: None } })
        }
        "status/error" => Some(rsx! {
            StatusBar {
                state: studio_state_error(),
                running: false,
                error: Some("Browser worker did not respond before the startup timeout.".to_string())
            }
        }),
        _ => None,
    }
}
