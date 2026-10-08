//! Offers: every verb the user can press, addressed by a stable path.
//!
//! Core builds each offer and publishes it into one [`UiOfferTree`] beside
//! the view (`UiStudioView::offers`). The web renders a surface's buttons
//! from the tree, and the app agent reads and presses the same tree, so the
//! two consumers can never disagree about what is offered. See
//! `docs/adr/2026-10-01-agentic-control-offers-in-core.md`.
//!
//! A verb that needs values first (flash *which board*) declares
//! [`OfferParam`]s; a press carries [`OfferArgs`], and the offer's
//! [`OfferBinder`] turns them into the action ([`UiOffer::press`]).

pub mod offer_arg_error;
pub mod offer_args;
pub mod offer_binder;
pub mod offer_param;
pub mod offer_path;
pub mod offer_press;
pub mod offer_search;
pub mod offer_shown_options;
pub mod ui_offer;
pub mod ui_offer_focus;
pub mod ui_offer_tree;

pub use offer_arg_error::OfferArgError;
pub use offer_args::OfferArgs;
pub use offer_binder::OfferBinder;
pub use offer_param::{OfferChoice, OfferParam, OfferParamKind};
pub use offer_path::{OfferPath, OfferPathError};
pub use offer_press::{OfferPress, SECRET_MARKER};
pub use offer_shown_options::FILTER_FINDS_NOTHING;
pub use ui_offer::UiOffer;
pub use ui_offer_focus::{OfferNearness, UiOfferFocus};
pub use ui_offer_tree::UiOfferTree;
