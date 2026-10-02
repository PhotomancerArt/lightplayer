//! Offers: every verb the user can press, addressed by a stable path.
//!
//! Core builds each offer and publishes it into one [`UiOfferTree`] beside
//! the view (`UiStudioView::offers`). The web renders a surface's buttons
//! from the tree, and the app agent reads and presses the same tree, so the
//! two consumers can never disagree about what is offered. See
//! `docs/adr/2026-10-01-agentic-control-offers-in-core.md`.

pub mod offer_path;
pub mod ui_offer;
pub mod ui_offer_tree;

pub use offer_path::{OfferPath, OfferPathError};
pub use ui_offer::UiOffer;
pub use ui_offer_tree::UiOfferTree;
