//! [`UiDrawable`]: a live picture a finger draws on, as the view says it.

use super::{UiDrawSurface, UiDrawTarget};

/// The fact that a picture is drawable, and what a stroke on it does.
///
/// A fact, not a verb: there is no offer and no action. The web reports a
/// stroke on [`Self::surface`] as [`crate::StudioCommand::Stroke`], and core
/// writes the touch set to [`Self::target`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiDrawable {
    pub surface: UiDrawSurface,
    pub target: UiDrawTarget,
}
