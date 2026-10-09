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
//! - Types: [`ui_board_card`], [`ui_board_picture`], [`ui_status_corner`],
//!   [`ui_name_bar`], [`ui_stack_bar`], [`ui_bar_work`], [`ui_card_action`],
//!   [`ui_detail_panel`].
//! - Building: [`board_card_input`], [`board_card_builder`], and one file
//!   per decision: [`board_picture`], [`status_corner`], [`primary_action`],
//!   [`bar_work`], and one per bar — [`project_bar`], [`connection_bar`],
//!   [`access_bar`], [`hardware_bar`] — with [`ui_bluetooth_switch`].

pub mod access_bar;
pub mod bar_work;
pub mod board_card_builder;
pub mod board_card_input;
pub mod board_picture;
#[cfg(test)]
pub(crate) mod card_fixtures;
pub mod connection_bar;
pub(crate) mod detail_sections;
pub mod hardware_bar;
pub mod primary_action;
pub mod project_bar;
pub mod status_corner;
pub mod ui_bar_work;
pub mod ui_bluetooth_switch;
pub mod ui_board_card;
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
pub use board_picture::LOCKED_PREVIEW_SENTENCE;
pub use connection_bar::SOMEONE_ELSE_SENTENCE;
pub use hardware_bar::chip_words;
pub use primary_action::NOTHING_TO_EDIT;
pub use ui_bar_work::{BarWorkState, UiBarWork};
pub use ui_bluetooth_switch::{UiBluetoothSwitch, bluetooth_switch};
pub use ui_board_card::{UiBoardCard, UiBoardPresence};
pub use ui_board_picture::{PictureSource, UiBoardPicture};
pub use ui_card_action::{UiActionDraw, UiCardAction};
pub use ui_detail_panel::UiDetailPanel;
pub use ui_name_bar::{UiNameBar, UiPrimary};
pub use ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
pub use ui_status_corner::{CornerMark, UiCornerDetails, UiStatusCorner};
