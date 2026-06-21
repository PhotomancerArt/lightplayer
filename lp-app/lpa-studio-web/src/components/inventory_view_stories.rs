use dioxus::prelude::*;

use crate::components::inventory_view::InventoryView;
use crate::stories::story::StoryDescriptor;
use crate::stories::story_fixtures::{studio_state_idle, studio_state_ready};

pub const STORIES: &[StoryDescriptor] = &[
    StoryDescriptor::new(
        "inventory/empty",
        "InventoryView",
        "Empty",
        "No project inventory has been read.",
    ),
    StoryDescriptor::new(
        "inventory/demo",
        "InventoryView",
        "Demo Project",
        "A populated demo inventory with nodes, definitions, and an asset.",
    ),
];

pub fn render_story(id: &str) -> Option<Element> {
    match id {
        "inventory/empty" => Some(rsx! { InventoryView { state: studio_state_idle() } }),
        "inventory/demo" => Some(rsx! { InventoryView { state: studio_state_ready() } }),
        _ => None,
    }
}
