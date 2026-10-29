//! The line to draw, as segments any renderer understands.
//!
//! Move, line and cubic are what SVG, canvas 2D and every vector API share,
//! so a renderer walks [`Path::segments`] or, for SVG, takes
//! [`Path::to_svg`] as a `d` attribute.

use core::fmt::Write;

use crate::{Cubic, Polyline, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(Vec2),
    LineTo(Vec2),
    CubicTo { c1: Vec2, c2: Vec2, to: Vec2 },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    pub segments: Vec<Segment>,
}

/// Each curve is cut into this many straight pieces when flattened.
pub(crate) const CURVE_PIECES: usize = 64;

impl Path {
    /// The path of one cubic: a `LineTo` when `straight`.
    pub fn of_cubic(c: &Cubic, straight: bool) -> Path {
        let last = if straight {
            Segment::LineTo(c.end)
        } else {
            Segment::CubicTo {
                c1: c.c1,
                c2: c.c2,
                to: c.end,
            }
        };
        Path {
            segments: vec![Segment::MoveTo(c.start), last],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.len() < 2
    }

    /// The `d` attribute of an SVG `<path>`, numbers to two decimals.
    pub fn to_svg(&self) -> String {
        let mut d = String::new();
        for s in &self.segments {
            if !d.is_empty() {
                d.push(' ');
            }
            match s {
                Segment::MoveTo(p) => {
                    let _ = write!(d, "M{}", pt(*p));
                }
                Segment::LineTo(p) => {
                    let _ = write!(d, "L{}", pt(*p));
                }
                Segment::CubicTo { c1, c2, to } => {
                    let _ = write!(d, "C{} {} {}", pt(*c1), pt(*c2), pt(*to));
                }
            }
        }
        d
    }

    /// The path as straight pieces, measured along its length.
    pub fn polyline(&self) -> Polyline {
        let mut points: Vec<Vec2> = Vec::new();
        let mut at = Vec2::ZERO;
        for s in &self.segments {
            match *s {
                Segment::MoveTo(p) => {
                    at = p;
                    points.push(p);
                }
                Segment::LineTo(p) => {
                    at = p;
                    points.push(p);
                }
                Segment::CubicTo { c1, c2, to } => {
                    let c = Cubic::new(at, c1, c2, to);
                    points.extend(
                        (1..=CURVE_PIECES).map(|i| c.point(i as f64 / CURVE_PIECES as f64)),
                    );
                    at = to;
                }
            }
        }
        Polyline::new(points)
    }

    /// How far `p` is from the line: for hovering.
    pub fn distance_to(&self, p: Vec2) -> f64 {
        self.polyline().distance_to(p)
    }
}

/// A number for SVG: at most two decimals, no trailing zeros, no `-0`.
pub(crate) fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

pub(crate) fn pt(p: Vec2) -> String {
    format!("{},{}", num(p.x), num(p.y))
}
