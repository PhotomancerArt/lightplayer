//! The roster card vocabulary — what survives the device teardown.
//!
//! The device half of this module (the 19-state `RosterCardState`, its
//! evidence derivation, the device rich object, the firmware-update chip,
//! the affordance set and the status-circle spec) was deleted in M2 of the
//! device-model rebuild; the rebuilt model projects card state from its own
//! DTOs. What remains is the LIVE SIMULATOR's card, which shared the
//! vocabulary by accident rather than by need:
//!
//! - [`sim_card_state`]: the sim's two-row status vocabulary.
//! - [`sim_rich_object`]: the live sim session as a rich object (D36) —
//!   Health, Project, Danger zone.
//! - [`card_tabs`]: sections → the card's icon tabs (M7′ — the card as
//!   control panel). Generic over the affordance type; the sim card's tab
//!   row is built here.

pub mod card_tabs;
pub mod sim_card_state;
pub mod sim_rich_object;

pub use card_tabs::{CardTab, CardTabView, card_tabs};
pub use sim_card_state::SimCardState;
pub use sim_rich_object::{SimDetailAffordance, SimRichInput, sim_rich_object};

/// A board id's human name for card lines: the catalog's `display_name`
/// when the id is a known board, else the raw id verbatim — advisory
/// metadata may name a board this build's catalog doesn't carry (a future
/// board, a typo'd id), and the line should still say something rather
/// than disappear. Same rule as the project card's "for \<board\>" badge.
pub fn board_display_name(board_id: &str) -> String {
    lpa_boards::board_by_id(board_id)
        .map(|board| board.display_name.clone())
        .unwrap_or_else(|| board_id.to_string())
}
