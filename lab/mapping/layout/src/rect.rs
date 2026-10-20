//! An axis-aligned box: where a node sits, `min` top-left, `max`
//! bottom-right.

use crate::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    /// A box of `size` with its top-left at `at`.
    pub fn at(at: Vec2, size: Vec2) -> Self {
        Self::new(at, at + size)
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn centre(&self) -> Vec2 {
        Vec2::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    /// The same box moved by `by`.
    pub fn shifted(&self, by: Vec2) -> Rect {
        Rect::new(self.min + by, self.max + by)
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(
            Vec2::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            Vec2::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        )
    }

    /// Whether the two share any area. Touching edges don't count.
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

impl Default for Rect {
    /// A zero box at the origin.
    fn default() -> Self {
        Rect::new(Vec2::ZERO, Vec2::ZERO)
    }
}
