//! Stories for recursive slot record editors.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::config_record_fixture;
use crate::app::node::SlotRecordEditor;

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
