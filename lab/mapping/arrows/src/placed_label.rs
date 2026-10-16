//! A label that found a spot: its little box, and how to set its text.
//!
//! The renderer fills the box with the background, so the line seems to
//! pass behind the words, then sets the text at `text_at`, rotated by
//! `angle_deg`, with the given anchor and baseline.

use crate::path::num;
use crate::{OrientedBox, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLabel {
    /// The box behind the text, padding included.
    pub frame: OrientedBox,
    /// Where the text is anchored: the box's centre.
    pub text_at: Vec2,
    /// The text's rotation, clockwise in degrees (y down), always in
    /// (−90, 90] so it never reads upside down. Equal to the frame's.
    pub angle_deg: f64,
    pub anchor: TextAnchor,
    pub baseline: TextBaseline,
    /// How far along the arrow its centre sits, 0–1 (a label beside the
    /// arrow: 0.5).
    pub along: f64,
}

/// Which point of the text `text_at` names, along the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAnchor {
    /// SVG `text-anchor="middle"`, canvas `textAlign = "center"`.
    Middle,
}

/// Which line of the text `text_at` names, across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBaseline {
    /// The middle of the letters: SVG `dominant-baseline="central"`,
    /// canvas `textBaseline = "middle"`.
    Central,
}

impl PlacedLabel {
    /// An SVG `transform` turning the text (and its box) into place:
    /// `rotate(angle cx cy)`.
    pub fn svg_transform(&self) -> String {
        format!(
            "rotate({} {} {})",
            num(self.angle_deg),
            num(self.text_at.x),
            num(self.text_at.y)
        )
    }
}
