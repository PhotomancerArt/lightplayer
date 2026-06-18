use dioxus::prelude::*;

use crate::components::device_panel::DevicePanel;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{
    studio_state_connected, studio_state_connecting, studio_state_idle, studio_state_long_content,
};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "device/idle",
        "DevicePanel",
        "Idle",
        "No endpoint has been discovered.",
    ),
    StoryDescriptor::new(
        "device/starting",
        "DevicePanel",
        "Starting",
        "Endpoint discovery has started the local worker.",
    ),
    StoryDescriptor::new(
        "device/connected",
        "DevicePanel",
        "Connected",
        "A browser-worker session is connected.",
    ),
    StoryDescriptor::new(
        "device/long-session",
        "DevicePanel",
        "Long Session",
        "Long session identifiers should wrap cleanly.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "device/idle" => Some(device_story(studio_state_idle(), false)),
        "device/starting" => Some(device_story(studio_state_connecting(), true)),
        "device/connected" => Some(device_story(studio_state_connected(), false)),
        "device/long-session" => Some(device_story(studio_state_long_content(), false)),
        _ => None,
    }
}

fn device_story(state: lp_studio_core::StudioState, running: bool) -> Element {
    rsx! {
        DevicePanel {
            state,
            running,
            on_start_demo: move |_| {}
        }
    }
}
