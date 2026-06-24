//! Stories for project editor field primitives in context.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::ui_studio::story_fixtures::editor_primitives_story;

#[story]
pub(crate) fn editor_fields() -> Element {
    editor_primitives_story()
}
