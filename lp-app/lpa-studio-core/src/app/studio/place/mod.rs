//! Place: where the user is in Studio — the page, and what is open over
//! it — as the web reports it (roadmap D3: "the core knows but its read
//! only").
//!
//! The web owns navigation: the router turns the URL into a route, and
//! the chrome opens and closes its drawers and panels. Core never moves
//! the user. It only RECEIVES the result, as [`UiPlace`] on
//! `StudioCommand::Place` (every report, in order: a batch never folds a
//! move and a move back into "no move", which a waiting Edit reads), and
//! reads it: the app agent's readout leads with what the user is looking
//! at, ⌘K ranks what is near it first, and a connected session shows on its
//! card or in the editor (`ConnectedBoard::shows_editor`).
//!
//! What core already owns is not reported again: the focused node and
//! each node card's open sections (`NodeCardUiState`), and the patch
//! surface's one selection (`UiSelection`). Place is the rest — the facts
//! only the page can see.
//!
//! Not to be confused with `app::places`, which is where a PROJECT lives
//! (the library, a device).

pub mod ui_page;
pub mod ui_panel;
pub mod ui_place;

pub use ui_page::{UiPage, UiProjectView};
pub use ui_panel::{UiPanel, UiSessionSection};
pub use ui_place::UiPlace;
