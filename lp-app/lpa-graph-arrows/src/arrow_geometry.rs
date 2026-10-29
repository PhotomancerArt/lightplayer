//! An arrow, laid out: everything a renderer draws, nothing it decides.
//!
//! Draw `path` as a stroke, each head (filled when `closed`, else
//! stroked), then each placed label's box filled with the background and
//! its text on top. All of it is in the units the arrow was given in.

use crate::{HeadGeometry, LabelGeometry, Path, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct ArrowGeometry {
    /// The line, cut back where a head would otherwise poke through.
    pub path: Path,
    pub heads: Vec<HeadGeometry>,
    /// One per label of the arrow, in its order, placed or not.
    pub labels: Vec<LabelGeometry>,
}

impl ArrowGeometry {
    /// What it covers: line, heads and placed labels.
    pub fn bounds(&self) -> Option<Rect> {
        let line = Rect::around(self.path.polyline().points().iter().copied());
        let heads = self.heads.iter().filter_map(|h| h.bounds());
        let labels = self
            .labels
            .iter()
            .filter_map(|l| l.placed.map(|p| p.frame.bounds()));
        line.into_iter()
            .chain(heads)
            .chain(labels)
            .reduce(|a, b| a.union(&b))
    }
}
