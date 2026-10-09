//! Offline boards: the boards Studio remembers and cannot see, as cards
//! with their last picture, in the roster's last-seen order (core lists
//! them, `UiHomeSections::offline`; the page does not re-sort). Hidden when
//! there are none.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiHomeSection, UiHomeView};

use super::boards_section::BoardsSection;
use super::home_view_mode::HomeViewMode;

/// The section's element id: the walks scope a board's card to it.
pub(crate) const OFFLINE_BOARDS_ID: &str = "home-offline-boards";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OfflineBoards(
    home: UiHomeView,
    mode: HomeViewMode,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        BoardsSection {
            section: UiHomeSection::OfflineBoards,
            id: OFFLINE_BOARDS_ID,
            key_prefix: "offline",
            entries: home.sections.offline.clone(),
            devices: home.devices.clone(),
            projects: home.projects.clone(),
            examples: home.examples.clone(),
            mode,
            on_action,
        }
    }
}
