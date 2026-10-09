//! Projects (the Projects tab): every library project that is not a
//! pattern, newest saved first (core lists them, `UiHomeSections::projects`;
//! PD6). A project a board plays is here too, saying which boards play it,
//! so even an offline board's project keeps its Rename, Duplicate, Copy
//! link, Download zip and Delete, and still opens on a sim. Then the add
//! row.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeView};

use super::home_view_mode::HomeViewMode;
use super::page_section::PageSection;
use super::project_add_row::ProjectAddRow;
use super::project_items::ProjectItems;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectsTabSection(
    home: UiHomeView,
    mode: HomeViewMode,
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        PageSection { title: UiHomeSection::Projects.label(),
            ProjectItems {
                uids: home.sections.projects.clone(),
                key_prefix: "project",
                projects: home.projects.clone(),
                opening: home.opening.clone(),
                mode,
                now_secs,
                on_action,
            }
            ProjectAddRow { busy: home.opening.is_some(), on_action }
        }
    }
}
