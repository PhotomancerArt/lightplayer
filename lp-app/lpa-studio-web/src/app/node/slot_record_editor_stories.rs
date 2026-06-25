//! Stories for recursive slot record editors.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::{asset_slots_fixture, config_record_fixture};
use crate::app::node::{ConfigSlotRow, SlotRecordEditor};

#[story(
    description = "A record editor with scalar fields and one collapsed top-level nested record."
)]
pub(crate) fn gallery() -> Element {
    rsx! {
        SlotRecordEditor { record: config_record_fixture() }
    }
}

#[story(description = "The same record body rendered as an already-open nested record.")]
pub(crate) fn nested_record() -> Element {
    rsx! {
        SlotRecordEditor {
            record: config_record_fixture(),
            depth: 1,
        }
    }
}

#[story(description = "An expanded asset slot with an editor-like GLSL preview.")]
pub(crate) fn asset_editor() -> Element {
    let asset = asset_slots_fixture().remove(0);

    rsx! {
        ConfigSlotRow {
            slot: asset,
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
        }
    }
}
