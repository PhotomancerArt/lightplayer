//! The board card's web pieces (`docs/style/ui.md` "The board card"; the
//! decision `docs/adr/2026-10-08-the-board-card-and-one-home-page.md` §2;
//! the look as decided `spikes/board-card-stack/index.html`).
//!
//! Core builds the card ([`lpa_studio_core::UiBoardCard`]): every word,
//! every tone and every offer on it. These pieces draw any card and decide
//! only its look — never what a bar says, its tone, or which offer it
//! carries (`docs/adr/2026-10-01-agentic-control-offers-in-core.md`). No
//! piece here builds a user verb's action: a button is an offer pressed by
//! its path ([`CardAction`]).
//!
//! | piece | draws |
//! |---|---|
//! | [`BoardCard`] | the card: the picture's row, the name bar, the five bars, at one height in every state |
//! | [`BoardPicture`](board_picture::BoardPicture) | the board's lights, dimmed when last known, or the update's light strip |
//! | [`StatusCorner`](status_corner::StatusCorner) | the notch cut out of the picture: the mark and the reading; it opens the corner's details |
//! | [`NameBar`](name_bar::NameBar) | the name, its place, the one primary as a flush section |
//! | [`StackBar`](stack_bar::StackBar) | one 28 px bar: icon, summary, aside (one trigger for its details), its action outside the trigger |
//! | [`BarWork`](bar_work::BarWork) | a bar's work: spinner and words with the iridescent foot, green when done, striped when failed |
//! | [`BarDetails`](bar_details::BarDetails) | a bar's (or the corner's) details: the merged detail card, its sections, panels and danger zone |
//! | [`BarDetailPanel`](bar_detail_panel::BarDetailPanel) | one of today's card surfaces inside a details card |
//! | [`CardAction`] | one action from the offer it presses, in the card's word and icon |
//!
//! # Walk hooks
//!
//! The card's DOM carries what a walk reads, in one place (P09 and later
//! walks read them here):
//!
//! - `data-board-card="devices/<board ref>"` on the card's root;
//! - `data-board-corner="fine|attention|warning|error|quiet|blank"` on the
//!   status corner, whose details hold the board's terminal;
//! - `data-bar="project|connection|access|firmware|hardware"` on each bar;
//! - `data-bar-work="running|done|failed"` on a bar while it carries work;
//! - `data-offer-path="devices/<board ref>/<verb>"` on every action
//!   (`AgentMark`), so a walk presses an offer by its path.
//!
//! Nothing in the app mounts the card yet (plan P08): it is drawn by its
//! stories ([`board_card_stories`]). The module is public so its pieces
//! are not dead code until then.

pub mod bar_detail_panel;
pub mod bar_details;
pub mod bar_work;
pub mod board_card;
#[cfg(feature = "stories")]
pub(crate) mod board_card_stories;
pub mod board_picture;
pub mod card_action;
#[cfg(test)]
pub(crate) mod card_test_fixtures;
pub mod name_bar;
pub mod stack_bar;
pub mod status_corner;

pub use board_card::{BoardCard, CardPart};
pub use card_action::{CardAction, CardActionLook, OfferAction};
