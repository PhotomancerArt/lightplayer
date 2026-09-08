//! What survives the device teardown: the board-name helper.
//!
//! The device half of this module (the 19-state `RosterCardState`, its
//! evidence derivation, the device rich object, the firmware-update chip,
//! the affordance set and the status-circle spec) was deleted in M2 of the
//! device-model rebuild; the rebuilt model projects card state from its own
//! DTOs. The SIM half (`sim_card_state`, `sim_rich_object`, `card_tabs`)
//! went with the sim card itself (PD9): a sim is a device, and a device's
//! card is `DeviceView`.

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
