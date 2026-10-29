//! What's drawn over the cards at one size on screen, in **screen
//! pixels**: an outline is a thing's area, the same shape a click hits
//! (`area.rs`), and areas are padded in pixels, so core gives them in
//! pixels too. The page draws them as they come.

use crate::domain::geom::{Rect, Vec2};
use crate::ux::ui::UiShape;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiOverlay {
    pub outlines: Vec<UiOutline>,
    pub handles: Vec<UiHandle>,
    /// The selection box, while one is dragged.
    pub selection_box: Option<Rect>,
    /// What's being drawn, before it exists.
    pub draft: Vec<UiDraftObject>,
    /// The points of a path being clicked out.
    pub draft_points: Vec<Vec2>,
    /// Lamps ringed: every lamp a selected port carries, and the lamp on a
    /// selected pixel (D47).
    pub marked_lamps: Vec<Vec2>,
}

/// One thing's area, outlined.
#[derive(Debug, Clone, PartialEq)]
pub struct UiOutline {
    /// The area as an SVG path (`Area::outline_path`).
    pub path: String,
    /// A path's area is its polyline stroked this wide; `None` for a
    /// closed outline.
    pub stroke: Option<f64>,
    pub kind: OutlineKind,
    /// Its name, and the top of its area to put it at.
    pub label: Option<(String, Vec2)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineKind {
    /// The level you are working in (the selection's parent): dashed.
    Context,
    Selected,
    /// What a click would select.
    Hover,
}

/// A handle on the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct UiHandle {
    pub rect: Rect,
    /// Hovered or being dragged.
    pub on: bool,
}

/// One object of the drawing in progress, with its lamps.
#[derive(Debug, Clone, PartialEq)]
pub struct UiDraftObject {
    pub shape: UiShape,
    /// Each lamp, and whether it lands inside the fixture it is drawn in.
    pub lamps: Vec<(Vec2, bool)>,
    pub arrow: Option<(Vec2, Vec2)>,
}
