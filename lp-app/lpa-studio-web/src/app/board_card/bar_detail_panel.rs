//! [`BarDetailPanel`]: one of today's card surfaces inside a details card
//! ([`UiDetailPanel`]), drawn as sections of that card — never a framed box
//! of its own.
//!
//! This phase draws the board's terminal ([`DeviceTerminal`]) flush in the
//! status corner's details; every other panel is a named placeholder until
//! it moves in (P07).

use dioxus::prelude::*;
use lpa_studio_core::{OfferPath, UiAction, UiDetailPanel};

use crate::app::home::device_terminal::DeviceTerminal;
use crate::base::DetailSection;

/// One panel. See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BarDetailPanel(
    panel: UiDetailPanel,
    /// `devices/<board ref>`: the board the panel acts on.
    board: OfferPath,
    on_action: EventHandler<UiAction>,
) -> Element {
    let _ = (&board, on_action);
    match panel {
        UiDetailPanel::Terminal { lines, dropped } => rsx! {
            DeviceTerminal { lines, dropped, height_class: TERMINAL_HEIGHT_CLASS }
        },
        other => rsx! {
            DetailSection { title: Some(placeholder_name(&other).to_string()) }
        },
    }
}

/// The terminal's fixed height inside a details card: a long log scrolls,
/// never grows the card.
pub(crate) const TERMINAL_HEIGHT_CLASS: &str = "tw:h-40";

/// A placeholder's name, until the panel moves in.
fn placeholder_name(panel: &UiDetailPanel) -> &'static str {
    match panel {
        UiDetailPanel::Terminal { .. } => "Terminal",
        UiDetailPanel::Access(_) => "Access",
        UiDetailPanel::Bluetooth(_) => "Bluetooth",
        UiDetailPanel::Wifi(_) => "Wi\u{2011}Fi",
        UiDetailPanel::Rename { .. } => "Rename",
        UiDetailPanel::LinkCounters(_) => "Link",
        UiDetailPanel::Layout(_) => "Before the files move",
        UiDetailPanel::OtherVersion { .. } => "Other version",
        UiDetailPanel::RestoreFromFile { .. } => "Restore from a backup file",
    }
}
