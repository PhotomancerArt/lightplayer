//! An arrowhead, drawn: a polygon with its tip on the arrow's end.

use crate::path::pt;
use crate::{HeadShape, Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowEnd {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadGeometry {
    pub end: ArrowEnd,
    pub shape: HeadShape,
    pub tip: Vec2,
    /// A triangle: tip and the two base corners, to fill. A chevron: one
    /// barb, the tip, the other barb, to stroke.
    pub points: Vec<Vec2>,
    /// Fill it (a triangle) rather than stroke it open (a chevron).
    pub closed: bool,
}

impl HeadGeometry {
    /// A head of `length` × `width` whose tip is at `tip`, pointing along
    /// the unit vector `toward` (the direction of travel into the tip).
    pub fn new(
        end: ArrowEnd,
        shape: HeadShape,
        tip: Vec2,
        toward: Vec2,
        length: f64,
        width: f64,
    ) -> Self {
        let base = tip - toward * length;
        let side = toward.perp() * (width / 2.0);
        let points = match shape {
            HeadShape::Triangle => vec![tip, base + side, base - side],
            HeadShape::Chevron => vec![base + side, tip, base - side],
        };
        HeadGeometry {
            end,
            shape,
            tip,
            points,
            closed: shape == HeadShape::Triangle,
        }
    }

    /// The head as an SVG path `d`: closed for a triangle, open for a
    /// chevron.
    pub fn to_svg(&self) -> String {
        let mut d = String::new();
        for (i, p) in self.points.iter().enumerate() {
            d.push_str(if i == 0 { "M" } else { " L" });
            d.push_str(&pt(*p));
        }
        if self.closed {
            d.push_str(" Z");
        }
        d
    }

    pub fn bounds(&self) -> Option<Rect> {
        Rect::around(self.points.iter().copied())
    }
}
