//! The board card, built in core
//! (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`): one board as
//! a picture, a status corner, a name bar with one primary action, and five
//! bars — project, connection, access, firmware, hardware — each with its
//! summary, aside, tone, one action, work in progress and details.
//!
//! The card is data. Its builder reads the roster's facts and the board's
//! own offers ([`BoardCardInput`]) and decides every word, tone and action;
//! the web only draws it, so the app agent sees the same card and presses
//! the same offers.
//!
//! Connected (the ADR's §4), the card carries the board's panel in the five
//! bars' place ([`ui_board_panel`]), picked from the project's root panel
//! ([`board_panel_picks`]), and its primary is Done; where the session
//! stands on each board is [`board_connection`].
//!
//! - Types: [`ui_board_card`], [`ui_board_picture`], [`ui_status_corner`],
//!   [`ui_name_bar`], [`ui_stack_bar`], [`ui_bar_work`], [`ui_card_action`],
//!   [`ui_detail_panel`], [`ui_board_panel`], [`board_connection`].
//! - Building: [`board_card_input`], [`board_card_builder`], and one file
//!   per decision: [`board_picture`], [`status_corner`], [`primary_action`],
//!   [`bar_work`], [`held_board`] (a board another tab holds),
//!   [`board_panel_picks`], and one per bar — [`project_bar`],
//!   [`connection_bar`], [`access_bar`], [`firmware_bar`],
//!   [`hardware_bar`] — with [`ui_bluetooth_switch`];
//!   [`roster_board_cards`] builds every card on a roster.

pub mod access_bar;
pub mod bar_work;
pub mod board_card_builder;
pub mod board_card_input;
pub mod board_connection;
pub mod board_panel_picks;
pub mod board_picture;
#[cfg(test)]
pub(crate) mod card_fixtures;
pub mod connection_bar;
pub(crate) mod detail_sections;
pub mod firmware_bar;
pub mod hardware_bar;
pub mod held_board;
pub mod primary_action;
pub mod project_bar;
pub mod roster_board_cards;
pub mod status_corner;
pub mod ui_bar_work;
pub mod ui_bluetooth_switch;
pub mod ui_board_card;
pub mod ui_board_panel;
pub mod ui_board_picture;
pub mod ui_card_action;
pub mod ui_detail_panel;
pub mod ui_name_bar;
pub mod ui_stack_bar;
pub mod ui_status_corner;

pub use access_bar::ANYONE_CAN_EDIT_SENTENCE;
pub use bar_work::{activity_bar, activity_words};
pub use board_card_builder::{board_card, pending_board_card};
pub use board_card_input::{BoardCardInput, link_icon};
pub use board_connection::BoardConnection;
pub use board_panel_picks::{BOARD_PANEL_CONTROLS, MASTER_CHANNEL, board_panel_picks};
pub use board_picture::{LENS_SOURCE_WORDS, LOCKED_PREVIEW_SENTENCE};
pub use connection_bar::{COULDNT_CONNECT, RECONNECTING, SOMEONE_ELSE_SENTENCE};
pub use hardware_bar::chip_words;
pub use held_board::{
    EDITOR_OPEN_ASIDE, HELD_NO_PICTURE_LINE, HELD_PICTURE_LINE, OPEN_IN_ANOTHER_TAB,
    TAKEN_BY_ANOTHER_TAB,
};
pub use primary_action::CONNECTING;
pub use roster_board_cards::{RosterCardsInput, roster_board_card, roster_board_cards};
pub use ui_bar_work::{BarWorkState, UiBarWork};
pub use ui_bluetooth_switch::{UiBluetoothSwitch, bluetooth_switch};
pub use ui_board_card::{UiBoardCard, UiBoardPresence};
pub use ui_board_panel::{UiBoardPanel, UiCardControl};
pub use ui_board_picture::{PictureSource, UiBoardPicture};
pub use ui_card_action::{UiActionDraw, UiCardAction};
pub use ui_detail_panel::UiDetailPanel;
pub use ui_name_bar::{UiNameBar, UiPrimary};
pub use ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
pub use ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
