//! [`OfferBinder`]: how a parameterised offer turns a press's values into
//! the operation it dispatches.

use core::fmt;
use std::rc::Rc;

use crate::{OfferArgError, OfferArgs, UiAction};

/// Turns a press's values into the action it dispatches.
///
/// Core builds one per parameterised offer, capturing what it already
/// resolved (the device, the boards a chip fits), so a renderer and the
/// agent hand over only the values the user picked. A binder sees args
/// that [`crate::UiOffer::press`] already checked against the offer's
/// parameters, with defaults filled in.
#[derive(Clone)]
pub struct OfferBinder(Rc<dyn Fn(&OfferArgs) -> Result<UiAction, OfferArgError>>);

impl OfferBinder {
    /// A binder that runs `bind`.
    pub fn new(bind: impl Fn(&OfferArgs) -> Result<UiAction, OfferArgError> + 'static) -> Self {
        Self(Rc::new(bind))
    }

    /// Bind `args` into the action to dispatch.
    pub fn bind(&self, args: &OfferArgs) -> Result<UiAction, OfferArgError> {
        (self.0)(args)
    }
}

impl fmt::Debug for OfferBinder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OfferBinder(..)")
    }
}
