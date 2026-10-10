//! Other projects (the All tab): the library projects no board plays,
//! newest saved first (core lists them, `UiHomeSections::other_projects`),
//! then the add row. Never hidden while the library is there: with nothing
//! in it, the add row is the whole section (a newcomer's only projects
//! line).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeView};

use super::home_view_mode::HomeViewMode;
use super::page_section::PageSection;
use super::project_add_row::ProjectAddRow;
use super::project_items::ProjectItems;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OtherProjects(
    home: UiHomeView,
    mode: HomeViewMode,
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        PageSection { title: UiHomeSection::OtherProjects.label(),
            ProjectItems {
                uids: home.sections.other_projects.clone(),
                key_prefix: "other",
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
