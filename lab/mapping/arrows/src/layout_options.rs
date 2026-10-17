//! The knobs of label layout, all in the caller's units.
//!
//! The defaults suit the lab's canvas at screen scale (CSS pixels): 10-unit
//! primary text, 9-unit quiet text.

use crate::{LabelRole, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    pub font_size_primary: f64,
    pub font_size_quiet: f64,
    /// Between the text and its box's edge: x along the text, y across.
    pub padding: Vec2,
    /// A label's box keeps at least this far from the arrow's ends and its
    /// heads.
    pub clearance: f64,
    /// How far the line may stray from straight under a label, as a
    /// fraction of half the box's height: 0.75 keeps the line inside the
    /// middle three quarters of the box, where the text hides it, and
    /// leaves it meeting the box's ends within a few degrees of the text.
    pub straightness: f64,
    /// Between two labels' boxes.
    pub gap: f64,
    /// When no stretch of the line is straight and long enough, try a
    /// level box beside the arrow's midpoint before dropping the label.
    pub beside: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            font_size_primary: 10.0,
            font_size_quiet: 9.0,
            padding: Vec2::new(4.0, 2.0),
            clearance: 4.0,
            straightness: 0.75,
            gap: 2.0,
            beside: true,
        }
    }
}

impl LayoutOptions {
    pub fn font_size(&self, role: LabelRole) -> f64 {
        match role {
            LabelRole::Primary => self.font_size_primary,
            LabelRole::Quiet => self.font_size_quiet,
        }
    }
}
