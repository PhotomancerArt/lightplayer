//! An axis-aligned rectangle: what an arrow covers, for culling and
//! fitting a view. Labels are not axis-aligned; they are
//! [`OrientedBox`](crate::OrientedBox)es.

use crate::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    /// The smallest rectangle holding every point; `None` for no points.
    pub fn around(points: impl IntoIterator<Item = Vec2>) -> Option<Rect> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut r = Rect {
            min: first,
            max: first,
        };
        for p in it {
            r.min = Vec2::new(r.min.x.min(p.x), r.min.y.min(p.y));
            r.max = Vec2::new(r.max.x.max(p.x), r.max.y.max(p.y));
        }
        Some(r)
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn union(&self, o: &Rect) -> Rect {
        Rect {
            min: Vec2::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            max: Vec2::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}
