//! Stories for the Projects header's New template menu.
//!
//! The menu is only ever seen open, so the baseline captures it open —
//! and beside the Import / Paste chips it shares the header with, because
//! the thing worth checking is that the trigger still reads as one of
//! three peers rather than a new kind of control.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::home::new_project_menu::NewProjectMenu;
use crate::app::home::section_title_class;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::quiet_action_class;

#[story(
    description = "The New menu open on the Projects header: three template rows, each a title over a dim one-liner. Blank is what `New` has always meant and stays first; the two pattern rows say what rig they build AND that they export `effect/`, so the export boundary is legible before the project exists. Text-first by design — the spike's visual template cards fight the 320px detail-card cap, and the picker's flat-list grammar is what the rest of Studio's menus use."
)]
pub(crate) fn menu_open() -> Element {
    header(true)
}

#[story(
    description = "The same header at rest: the New trigger keeps the quiet-chip look it shares with Import and Paste, so what changed is only what it opens."
)]
pub(crate) fn trigger_at_rest() -> Element {
    header(false)
}

/// The Projects header strip, with the New menu optionally open. Mirrors
/// `ProjectsPage`'s own header row so the story cannot drift from the
/// shipped layout without someone noticing.
fn header(open: bool) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-h-[320px] tw:content-start tw:gap-3 tw:p-4",
            header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                h2 { class: section_title_class(), "Projects" }
                div { class: "tw:flex tw:items-center tw:gap-2",
                    NewProjectMenu { initially_open: open, on_action: |_| {} }
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Upload, size: 14 }
                        }
                        span { "Import" }
                    }
                    button {
                        class: quiet_action_class(),
                        r#type: "button",
                        span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                            StudioIcon { name: StudioIconName::Copy, size: 14 }
                        }
                        span { "Paste" }
                    }
                }
            }
        }
    }
}
