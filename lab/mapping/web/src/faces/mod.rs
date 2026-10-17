//! Faces: what each node shows on the canvas, drawn from the view tree in
//! card-local world units. The world layer's transform zooms them; things
//! that must stay one size on screen are sized by the face's `zoom`.

mod card_text;
mod fixture_face;
mod output_face;
mod plain_face;
mod source_face;

pub use card_text::{CardTitle, card_font};
pub use fixture_face::FixtureFace;
pub use output_face::OutputFace;
pub use plain_face::PlainFace;
pub use source_face::SourceFace;

use lab_mapping_model::{Emphasis, Rgb};

/// A lamp's radius on screen: it grows with zoom, within limits, so it
/// stays readable.
pub fn lamp_radius_px(zoom: f64) -> f64 {
    (zoom * 0.55).clamp(1.8, 6.0)
}

/// The class every element with this emphasis wears: one style each.
pub fn emphasis_class(e: Emphasis) -> &'static str {
    match e {
        Emphasis::Full => "emph-full",
        Emphasis::Lit => "emph-lit",
        Emphasis::Quiet => "emph-quiet",
        Emphasis::Dimmed => "emph-dimmed",
    }
}

/// How much of itself something with this emphasis shows, on a canvas
/// where classes don't reach. Matches `lab.css`.
pub fn emphasis_alpha(e: Emphasis) -> f64 {
    match e {
        Emphasis::Dimmed => 0.3,
        _ => 1.0,
    }
}

pub fn rgb(c: Rgb) -> [f64; 4] {
    [c.r as f64, c.g as f64, c.b as f64, 1.0]
}
