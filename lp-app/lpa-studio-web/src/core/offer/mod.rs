//! The offer tree on the web side: where it is provided, and how a surface
//! asks it for its verbs.

pub mod offers_context;

pub use offers_context::{OffersProvider, use_offers, use_provide_offers, use_verbs_of};
