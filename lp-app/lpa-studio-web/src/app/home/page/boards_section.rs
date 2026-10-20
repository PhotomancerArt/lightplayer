//! The body Online boards and Offline boards share: the section's boards
//! as cards (the grid the Devices page used) or as rows (one bordered
//! list), each card mounted through [`BoardCardSlot`].

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceRosterView, UiAction, UiExampleCard, UiHomeBoard, UiHomeSection, UiPackageCard,
};

use super::board_card_slot::BoardCardSlot;
use super::board_row::BoardRow;
use super::home_view_mode::HomeViewMode;
use super::page_section::PageSection;
use crate::app::home::device_grid_class;

/// A titled section of boards, hidden when it holds none.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardsSection(
    /// Which section: its heading is core's word for it.
    section: UiHomeSection,
    /// The section's element id (the walks scope to it).
    id: &'static str,
    /// The prefix every key in this section's list carries, so a board can
    /// never share a key with one in another section.
    key_prefix: &'static str,
    /// The section's boards, in core's order.
    entries: Vec<UiHomeBoard>,
    devices: DeviceRosterView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    mode: HomeViewMode,
    on_action: EventHandler<UiAction>,
) -> Element {
    if entries.is_empty() {
        return rsx! {};
    }
    rsx! {
        PageSection { title: section.label(), id,
            match mode {
                HomeViewMode::Cards => rsx! {
                    div { class: device_grid_class(),
                        for entry in entries {
                            BoardCardSlot {
                                key: "{key_prefix}-{entry.id.0}",
                                entry,
                                devices: devices.clone(),
                                projects: projects.clone(),
                                examples: examples.clone(),
                                on_action,
                            }
                        }
                    }
                },
                HomeViewMode::List => rsx! {
                    div { class: LIST_CLASS,
                        for entry in entries {
                            BoardRow {
                                key: "{key_prefix}-{entry.id.0}",
                                entry,
                                devices: devices.clone(),
                                on_action,
                            }
                        }
                    }
                },
            }
        }
    }
}

/// The rows' frame: one bordered list, a hairline between rows.
const LIST_CLASS: &str = "tw:grid tw:divide-y tw:divide-border-muted tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card";
