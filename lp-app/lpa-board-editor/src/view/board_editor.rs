//! The editor body: form sections on the left, live preview + lint on the
//! right. Page chrome (open/save/paste) stays in the page host.

use dioxus::prelude::*;

use crate::editor_core::editor_doc::{EditorDoc, RailTarget};
use crate::view::drawing_form::DrawingSection;
use crate::view::identity_form::IdentitySection;
use crate::view::lint_panel::LintPanel;
use crate::view::pin_table::PinRailEditor;
use crate::view::preview_pane::PreviewPane;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardEditor(doc: Signal<EditorDoc>) -> Element {
    rsx! {
        div { class: "lpb-ed-body",
            div { class: "lpb-ed-form",
                IdentitySection { doc }
                DrawingSection { doc }
                PinRailEditor { doc, target: RailTarget::Terminals }
                PinRailEditor { doc, target: RailTarget::Left }
                PinRailEditor { doc, target: RailTarget::Right }
            }
            div { class: "lpb-ed-side",
                PreviewPane { doc }
                LintPanel { doc }
            }
        }
    }
}
