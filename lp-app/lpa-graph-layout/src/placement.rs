//! What a layout gives back: every node's box, and the box around them all.

use std::collections::BTreeMap;

use crate::{Rect, Vec2};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placement {
    /// Every node's box, in world units, the root's inside at `(0, 0)`.
    pub rects: BTreeMap<String, Rect>,
    /// Where each module's inside starts, in world units: what a pin in
    /// that module is measured from.
    pub insides: BTreeMap<String, Vec2>,
    /// Everything laid out; a zero box at the origin when there's nothing.
    pub extent: Rect,
}

impl Placement {
    pub fn rect(&self, id: &str) -> Option<Rect> {
        self.rects.get(id).copied()
    }

    /// Where the inside of `parent` starts: a module's from
    /// [`Placement::insides`], the root's at `(0, 0)`.
    pub fn inside(&self, parent: Option<&str>) -> Vec2 {
        parent
            .and_then(|p| self.insides.get(p).copied())
            .unwrap_or_default()
    }

    /// Fill in [`Placement::extent`] from the rects.
    pub(crate) fn with_extent(mut self) -> Self {
        self.extent = self
            .rects
            .values()
            .copied()
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        self
    }
}
