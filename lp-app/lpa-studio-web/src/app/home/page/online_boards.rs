//! Online boards: the boards Studio can see right now — a link still being
//! identified first, then the connected boards in the roster's order (core
//! lists them, `UiHomeSections::online`). Hidden when there are none.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeView};

use super::boards_section::BoardsSection;
use super::home_view_mode::HomeViewMode;

/// The section's element id: the walks scope a board's card to it.
pub(crate) const ONLINE_BOARDS_ID: &str = "home-online-boards";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OnlineBoards(
    home: UiHomeView,
    mode: HomeViewMode,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        BoardsSection {
            section: UiHomeSection::OnlineBoards,
            id: ONLINE_BOARDS_ID,
            key_prefix: "online",
            entries: home.sections.online.clone(),
            devices: home.devices.clone(),
            projects: home.projects.clone(),
            examples: home.examples.clone(),
            mode,
            on_action,
        }
    }
}
