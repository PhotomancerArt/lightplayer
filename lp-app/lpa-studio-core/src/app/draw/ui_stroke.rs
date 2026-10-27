//! [`UiStroke`]: the fingers on a drawable picture, as the web reports them.

use super::UiDrawSurface;

/// Where every finger on a picture is now (`StudioCommand::Stroke`).
///
/// A fact the web reports, like `StudioCommand::Place`, and never an
/// action: core owns the target, the conversion to pattern space, the pace,
/// the renewal and the release (plan D11). An empty `pointers` is "every
/// finger is up".
#[derive(Clone, Debug, PartialEq)]
pub struct UiStroke {
    /// The picture the fingers are on, as its [`crate::UiDrawable`] named it.
    pub surface: UiDrawSurface,
    /// Every finger down now. Empty = released.
    pub pointers: Vec<UiStrokePointer>,
}

/// One finger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiStrokePointer {
    /// The browser's own pointer id (`PointerEvent.pointerId`): any value,
    /// stable while the finger is down. Core gives the board its own small
    /// touch id.
    pub pointer: u32,
    /// Position as `0..1` of the drawn picture box, x right
    /// (`PictureFit::picture_point`). Outside `0..1` when the finger is off
    /// the picture: not clamped.
    pub x: f32,
    /// Position as `0..1` of the drawn picture box, y DOWN, as the screen.
    pub y: f32,
}

impl UiStroke {
    /// The fingers on `surface`; an empty list releases.
    #[must_use]
    pub fn new(surface: UiDrawSurface, pointers: Vec<UiStrokePointer>) -> Self {
        Self { surface, pointers }
    }

    /// Every finger on `surface` is up.
    #[must_use]
    pub fn released(surface: UiDrawSurface) -> Self {
        Self::new(surface, Vec::new())
    }
}
