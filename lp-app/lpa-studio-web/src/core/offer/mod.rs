//! The offer tree on the web side: where it is provided, and how a surface
//! asks it for its verbs.

pub mod offer_params_form;
#[cfg(feature = "stories")]
pub(crate) mod offer_params_form_stories;
pub mod offers_context;

pub use offer_params_form::{
    OfferParamsForm, OfferPressButton, pressed_or_refused, resolved_args, visible_options,
};
pub use offers_context::{
    OffersProvider, use_device_verbs, use_offer_at, use_offers, use_provide_offers, use_verbs_of,
    verb_named,
};
