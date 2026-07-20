//! The status-circle spec: shape × status family, renderer-independent.

use crate::UiStatusKind;

/// What a roster card's status circle should communicate. Renderers (the
/// web `StatusCircle` component today; on-device LEDs later) map this onto
/// their own medium.
///
/// Shape and motion carry meaning without color:
/// solid = live link, hollow = remembered only, pulsing = working.
/// The tone reuses the existing status families (green good, amber
/// attention, red broken, gray neutral) — no parallel color vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RosterCircle {
    pub shape: RosterCircleShape,
    pub tone: UiStatusKind,
}

/// The circle's shape grammar (direction.md "Card grammar").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RosterCircleShape {
    /// A live link exists to the thing this card describes.
    Solid,
    /// Remembered only — no live link (offline registry cards).
    Hollow,
    /// Work is in flight (connecting, flashing, pushing).
    Pulsing,
}
