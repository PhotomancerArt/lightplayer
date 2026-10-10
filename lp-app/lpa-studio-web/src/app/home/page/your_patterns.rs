//! Your patterns (the All and Patterns tabs): the library's pattern
//! projects, newest saved first (core lists them,
//! `UiHomeSections::patterns`). They draw as project cards (PQ16), whose ⋯
//! menu holds "New project from this…", a pattern's one verb. Hidden when
//! there are none.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeView};

use super::home_view_mode::HomeViewMode;
use super::page_section::PageSection;
use super::project_items::ProjectItems;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn YourPatterns(
    home: UiHomeView,
    mode: HomeViewMode,
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    if home.sections.patterns.is_empty() {
        return rsx! {};
    }
    rsx! {
        PageSection { title: UiHomeSection::YourPatterns.label(),
            ProjectItems {
                uids: home.sections.patterns.clone(),
                key_prefix: "pattern",
                projects: home.projects.clone(),
                opening: home.opening.clone(),
                mode,
                now_secs,
                on_action,
            }
        }
    }
}
