//! Areas: the shape of each thing on screen. One shape serves both what is
//! outlined and what a click hits, so **what's outlined is exactly what a
//! click hits.**
//!
//! - lamp: a small disc;
//! - line: a capsule around its segment;
//! - ring: a band around its circle;
//! - circle: a disc out past its outer ring;
//! - group: a rounded hull around its children — the shape you'd draw
//!   around them by hand.
//!
//! Areas are in screen pixels, because their padding is: a line is as easy
//! to hit zoomed out as zoomed in.

use crate::camera::Camera;
use crate::component::ComponentKind;
use crate::fixture::Fixture;
use crate::geom::{Vec2, distance_to_segment};
use crate::object::{ObjectShape, objects_of};
use crate::target::Target;

pub const LAMP_PAD_PX: f64 = 7.0;
pub const LINE_PAD_PX: f64 = 10.0;
pub const RING_PAD_PX: f64 = 8.0;
pub const CIRCLE_PAD_PX: f64 = 10.0;
pub const GROUP_PAD_PX: f64 = 14.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Area {
    /// Everything within `pad` of a convex hull (a capsule for two points, a
    /// disc for one). `points` IS the hull: build it with [`Area::hull`].
    Hull { points: Vec<Vec2>, pad: f64 },
    /// Everything within `pad` of a circle's line.
    Band { center: Vec2, radius: f64, pad: f64 },
}

impl Area {
    pub fn hull(points: &[Vec2], pad: f64) -> Area {
        Area::Hull {
            points: convex_hull(points),
            pad,
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            Area::Hull { points, pad } => hull_distance(points, p) <= *pad,
            Area::Band {
                center,
                radius,
                pad,
            } => (p.distance(*center) - radius).abs() <= *pad,
        }
    }

    /// How far `p` is from the thing itself (its lamps, its line, the inside
    /// of its hull) — "nearest wins" when areas overlap.
    pub fn distance(&self, p: Vec2) -> f64 {
        match self {
            Area::Hull { points, .. } => hull_distance(points, p),
            Area::Band { center, radius, .. } => (p.distance(*center) - radius).abs(),
        }
    }

    /// The top of the area: where a name label goes.
    pub fn top(&self) -> Vec2 {
        match self {
            Area::Hull { points, pad } => {
                let top = points
                    .iter()
                    .copied()
                    .min_by(|a, b| a.y.total_cmp(&b.y))
                    .unwrap_or_default();
                Vec2::new(top.x, top.y - pad)
            }
            Area::Band {
                center,
                radius,
                pad,
            } => Vec2::new(center.x, center.y - radius - pad),
        }
    }

    /// The outline as an SVG path. A band is two circles (fill it with
    /// `evenodd`).
    pub fn outline_path(&self) -> String {
        match self {
            Area::Hull { points, pad } => rounded_hull_path(points, *pad),
            Area::Band {
                center,
                radius,
                pad,
            } => {
                let outer = circle_path(*center, radius + pad);
                if *radius > *pad {
                    format!("{outer} {}", circle_path(*center, radius - pad))
                } else {
                    outer
                }
            }
        }
    }
}

impl Fixture {
    /// The area of one target on screen.
    pub fn area(&self, camera: &Camera, t: &Target) -> Option<Area> {
        let screen = |p: Vec2| camera.to_screen(p);
        match t {
            Target::Lamp { .. } => {
                let p = *self.lamps_under(t).first()?;
                Some(Area::hull(&[screen(p)], LAMP_PAD_PX))
            }
            Target::Object(o) => match self.object(o)?.shape {
                ObjectShape::Ring { center, radius } => Some(Area::Band {
                    center: screen(center),
                    radius: radius * camera.scale,
                    pad: RING_PAD_PX,
                }),
                ObjectShape::Segment { from, to } => {
                    Some(Area::hull(&[screen(from), screen(to)], LINE_PAD_PX))
                }
            },
            Target::Component(id) => {
                let c = self.component(id)?;
                match &c.kind {
                    ComponentKind::Line { from, to, .. } => {
                        Some(Area::hull(&[screen(*from), screen(*to)], LINE_PAD_PX))
                    }
                    ComponentKind::Circle { center, rings, .. } => {
                        let r = rings.iter().map(|r| r.radius).fold(0.0, f64::max) * camera.scale;
                        Some(Area::hull(&[screen(*center)], r + CIRCLE_PAD_PX))
                    }
                    ComponentKind::Group { .. } => {
                        let mut points: Vec<Vec2> =
                            self.lamps_under(t).into_iter().map(screen).collect();
                        // Circles inside a group count by their whole disc.
                        for o in collect_objects(self, c) {
                            if let ObjectShape::Ring { center, radius } = o.shape {
                                points.extend(ring_points(screen(center), radius * camera.scale));
                            }
                        }
                        (!points.is_empty()).then(|| Area::hull(&points, GROUP_PAD_PX))
                    }
                }
            }
        }
    }

    /// Everything whose area contains `p`, at every level, in tree order.
    /// The right-click menu, ⌥-click cycling and the hover note all read
    /// this.
    pub fn things_at(&self, camera: &Camera, p: Vec2) -> Vec<Target> {
        let mut out = Vec::new();
        for t in self.children(None) {
            self.collect_at(camera, p, &t, &mut out);
        }
        out
    }

    /// Every child's area lies inside its parent's (a lamp's disc inside its
    /// line's capsule, a ring's band inside its circle's disc, everything
    /// inside its group's hull), so a thing the cursor is outside of has
    /// nothing under the cursor inside it either.
    fn collect_at(&self, camera: &Camera, p: Vec2, t: &Target, out: &mut Vec<Target>) {
        if !self.area(camera, t).is_some_and(|a| a.contains(p)) {
            return;
        }
        out.push(t.clone());
        for child in self.children(Some(t)) {
            self.collect_at(camera, p, &child, out);
        }
    }

    /// How far `p` is from `t` on screen (`INFINITY` if it has no area).
    pub fn distance_to(&self, camera: &Camera, t: &Target, p: Vec2) -> f64 {
        self.area(camera, t)
            .map_or(f64::INFINITY, |a| a.distance(p))
    }
}

fn collect_objects(f: &Fixture, c: &crate::Component) -> Vec<crate::Object> {
    let mut out = objects_of(c);
    for child in c.children() {
        out.extend(collect_objects(f, child));
    }
    out
}

fn ring_points(center: Vec2, radius: f64) -> Vec<Vec2> {
    (0..24)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 24.0;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

fn cross(o: Vec2, a: Vec2, b: Vec2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// The convex hull, counter-clockwise in coordinate terms (Andrew's
/// monotone chain). Duplicates and collinear points are dropped.
pub fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if pts.len() < 3 {
        return pts;
    }
    let mut lower: Vec<Vec2> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Distance from `p` to a convex hull: zero inside it.
fn hull_distance(hull: &[Vec2], p: Vec2) -> f64 {
    match hull.len() {
        0 => f64::INFINITY,
        1 => p.distance(hull[0]),
        2 => distance_to_segment(p, hull[0], hull[1]),
        n => {
            let inside = (0..n).all(|i| cross(hull[i], hull[(i + 1) % n], p) >= 0.0);
            if inside {
                0.0
            } else {
                (0..n)
                    .map(|i| distance_to_segment(p, hull[i], hull[(i + 1) % n]))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }
}

fn circle_path(c: Vec2, r: f64) -> String {
    format!(
        "M{},{} a{r},{r} 0 1,0 {},0 a{r},{r} 0 1,0 {},0 Z",
        c.x - r,
        c.y,
        2.0 * r,
        -2.0 * r
    )
}

/// The hull pushed out by `pad`, with round corners: each edge moved out
/// along its normal, joined by an arc around each corner.
fn rounded_hull_path(hull: &[Vec2], pad: f64) -> String {
    match hull.len() {
        0 => String::new(),
        1 => circle_path(hull[0], pad),
        n => {
            let normal = |a: Vec2, b: Vec2| {
                let d = b - a;
                let len = d.length().max(1e-9);
                Vec2::new(d.y / len, -d.x / len)
            };
            // For a two-point "hull" the edges are a→b and b→a.
            let edges: Vec<(Vec2, Vec2)> = (0..n).map(|i| (hull[i], hull[(i + 1) % n])).collect();
            let mut d = String::new();
            for (i, (a, b)) in edges.iter().enumerate() {
                let (pa, _) = edges[(i + n - 1) % n];
                let n_prev = normal(pa, *a);
                let n_this = normal(*a, *b);
                let arc_start = *a + n_prev * pad;
                let arc_end = *a + n_this * pad;
                let edge_end = *b + n_this * pad;
                if i == 0 {
                    d.push_str(&format!("M{},{} ", arc_start.x, arc_start.y));
                }
                d.push_str(&format!("A{pad},{pad} 0 0,1 {},{} ", arc_end.x, arc_end.y));
                d.push_str(&format!("L{},{} ", edge_end.x, edge_end.y));
            }
            d.push('Z');
            d
        }
    }
}
