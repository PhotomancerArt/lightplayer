//! The house wheel grammar, shared by every canvas (gate ruling: one
//! control scheme via one code): **scroll pans, ⌘/Ctrl-scroll zooms**,
//! with one delta normalization (pixels / lines / pages) so a Magic
//! Mouse, a wheel, and a trackpad all read the same.

use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;

/// What a wheel event asks of the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelGesture {
    /// Scale about the pointer by `factor` (already exponent-shaped:
    /// multiply the current scale).
    Zoom { factor: f32 },
    /// Move the camera by `(dx, dy)` view pixels (natural direction —
    /// content follows the scroll).
    Pan { dx: f32, dy: f32 },
}

/// Classify a wheel event under the house grammar.
#[must_use]
pub fn wheel_gesture(event: &Event<WheelData>) -> WheelGesture {
    let (dx, dy) = normalized_delta(event.data().delta());
    let modifiers = event.data().modifiers();
    if modifiers.ctrl() || modifiers.meta() {
        WheelGesture::Zoom {
            factor: (1.004f32).powf(-dy),
        }
    } else {
        WheelGesture::Pan { dx: -dx, dy: -dy }
    }
}

/// One pixel-space reading for the three wheel delta units.
fn normalized_delta(delta: WheelDelta) -> (f32, f32) {
    match delta {
        WheelDelta::Pixels(v) => (v.x as f32, v.y as f32),
        WheelDelta::Lines(v) => (v.x as f32 * 16.0, v.y as f32 * 16.0),
        WheelDelta::Pages(v) => (v.x as f32 * 100.0, v.y as f32 * 100.0),
    }
}
