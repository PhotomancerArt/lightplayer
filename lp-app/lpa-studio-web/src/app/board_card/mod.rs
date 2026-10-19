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
//! | [`BoardCard`] | the card: the picture's row, the name bar, the five bars at one height in every state (connected, the board's whole panel in their place, as tall as it needs: [`CompactPanel`](crate::app::module::CompactPanel)) |
//! | [`BoardPicture`](board_picture::BoardPicture) | the board's lights, dimmed when last known, or the update's light strip |
//! | [`BoardSurface`](board_surface::BoardSurface) | over the picture, what matters now: the state's icon in its tone, one or two plain sentences, the work's progress; the picture dims under it |
//! | [`StatusCorner`](status_corner::StatusCorner) | the notch cut out of the picture: a quiet ⋯ that opens the corner's details |
//! | [`NameBar`](name_bar::NameBar) | the name, its place, the one primary as a flush section |
//! | [`StackBar`](stack_bar::StackBar) | one 28 px row in the card's grammar — icon · kind · name · state (one trigger for its details), its action outside the trigger; while it works, the spinner in the icon's place and the work's words as its state |
//! | [`WorkFoot`](bar_work::WorkFoot) | a row's running work: the iridescent fill along its foot |
//! | [`BarDetails`](bar_details::BarDetails) | a bar's (or the corner's) details: the merged detail card, its sections, panels and danger zone |
//! | [`BarDetailPanel`](bar_detail_panel::BarDetailPanel) | one of today's card surfaces inside a details card: the terminal, access, Wi‑Fi, link counters, rename, the layout question, other version, restore from file, the connected panel's settings |
//! | [`CardAction`] | one action from the offer it presses, in the card's word and icon |
//!
//! The panels moved here from today's card (`link_counters_section`,
//! `other_version_form`,
//! `restore_from_file_button`) are sections of the details card they sit
//! in: no box in a box, no popover in a popover.
//!
//! # Walk hooks
//!
//! The card's DOM carries what a walk reads, in one place (P09 and later
//! walks read them here):
//!
//! - `data-board-card="devices/<board ref>"` on the card's root;
//! - `data-surface="<state>"` on the surface over the picture
//!   (`old-firmware`, `offline`, `all-well`, … — core's
//!   `SurfaceState::as_str`), with `data-surface-first` and
//!   `data-surface-second` on its sentences and `data-surface-progress` on
//!   its bar;
//! - `data-board-corner="fine|quiet|blank"` on the status corner, a quiet
//!   ⋯ whose details hold the board's terminal;
//! - `data-bar="project|connection|access|firmware|hardware"` on each bar;
//! - `data-bar-work="running|done|failed"` on a bar while it carries work;
//! - `data-picture="link|lens|saved|none"` on the picture, with
//!   `data-picture-frame="true|false"` for whether it draws a frame: a board
//!   another tab holds shows `saved`, the picture that tab left behind;
//! - `data-offer-path="devices/<board ref>/<verb>"` on every action
//!   (`AgentMark`), so a walk presses an offer by its path;
//! - `data-board-terminal` on the board's terminal in the corner's details
//!   (`DeviceTerminal`): what the board said, in its own words;
//! - connected, in the bars' place: `data-board-panel` on the panel,
//!   `data-board-panel-heading` on a group's heading and
//!   `data-board-panel-foot` on its foot (Edit at its end); in the corner's
//!   details, `data-panel-reset` and `data-panel-auto-save`; each of the
//!   panel's controls carries the panel control's own
//!   `data-panel-scope`, `data-panel-channel` and
//!   `data-panel-state="read-default|read-following|engaged"`, as it does in
//!   the editor and on the play page (`ModulePanelControl`).
//!
//! A bar's details trigger is the button named "<Bar> details" ("Project
//! details", …), the corner's "Status details". Details render inside the
//! card's DOM (the popover's layer is the trigger's sibling, raised to the
//! top layer), so a walk scopes every read to its card. The helpers that
//! read these are `scripts/emu/studio-driver.mjs`'s card helpers.
//!
//! The card is mounted wherever a board appears: the home page's
//! `BoardCardSlot`, the editor's docked card (`LensCardPane`) and the
//! mismatch page. Its stories are [`board_card_stories`].

pub(crate) mod bar_detail_panel;
pub(crate) mod bar_details;
pub(crate) mod bar_work;
pub(crate) mod board_card;
#[cfg(feature = "stories")]
pub(crate) mod board_card_stories;
pub(crate) mod board_picture;
pub(crate) mod board_surface;
pub(crate) mod card_action;
#[cfg(test)]
pub(crate) mod card_test_fixtures;
pub(crate) mod link_counters_section;
pub(crate) mod name_bar;
pub(crate) mod other_version_form;
pub(crate) mod panel_settings_section;
pub(crate) mod restore_from_file_button;
pub(crate) mod stack_bar;
pub(crate) mod status_corner;

pub(crate) use board_card::BoardCard;
#[cfg(any(test, feature = "stories"))]
pub(crate) use board_card::CardPart;
#[cfg(feature = "stories")]
pub(crate) use card_action::CardPreviews;
pub(crate) use card_action::{CardAction, CardActionLook, OfferAction};
