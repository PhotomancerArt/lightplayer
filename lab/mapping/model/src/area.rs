//! Areas: the shape of each thing on screen. One shape serves both what is
//! outlined and what a click hits, so **what's outlined is exactly what a
//! click hits.**
//!
//! - lamp: a small disc;
//! - path: everything near its polyline — a capsule round each segment,
//!   joined with round corners (a two-point path is one capsule);
//! - ring: a band around its circle;
//! - polygon: everything near its closed outline;
//! - circle: a disc out past its outer ring;
//! - repeat: everything near any of its copies' outlines (each copy is
//!   its own strand, often far from the next);
//! - group: a rounded hull around its children — the shape you'd draw
//!   around them by hand;
//! - module, fixture: a frame — its border and title tab, not its inside;
//! - output (and any other card): its whole card; a port: its row on the
//!   card; a pixel: its square in the row (`pixel_grid.rs`).
//!
//! Areas are in screen pixels, because their padding is: a path is as easy
//! to hit zoomed out as zoomed in.

use crate::camera::Camera;
use crate::component::ComponentKind;
use crate::geom::{Rect, Vec2, distance_to_polyline, distance_to_segment};
use crate::object::ObjectShape;
use crate::project::Project;
use crate::target::Target;

pub const LAMP_PAD_PX: f64 = 7.0;
pub const PATH_PAD_PX: f64 = 10.0;
pub const RING_PAD_PX: f64 = 8.0;
pub const CIRCLE_PAD_PX: f64 = 10.0;
pub const GROUP_PAD_PX: f64 = 14.0;
/// A frame (module, fixture) is picked by its border, within this many pixels…
pub const FRAME_EDGE_PX: f64 = 5.0;
/// …or by its title tab above the box, this tall.
pub const FRAME_TITLE_PX: f64 = 18.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Area {
    /// Everything within `pad` of a convex hull (a capsule for two points, a
    /// disc for one). `points` IS the hull: build it with [`Area::hull`].
    Hull { points: Vec<Vec2>, pad: f64 },
    /// Everything within `pad` of a polyline: a path's shape. Not convex —
    /// an L-shaped path does not own the corner it turns round.
    Polyline { points: Vec<Vec2>, pad: f64 },
    /// Everything within `pad` of any of several polylines: a repeat's
    /// copies, each its own strand.
    Polylines { lines: Vec<Vec<Vec2>>, pad: f64 },
    /// Everything within `pad` of a circle's line.
    Band { center: Vec2, radius: f64, pad: f64 },
    /// A frame: its border and its title tab, NOT its inside — the inside
    /// belongs to what's in it, and to the selection box.
    Frame { rect: Rect, title: Rect },
    /// A filled rectangle: an output card, a port's row.
    Box { rect: Rect },
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
            Area::Polyline { points, pad } => distance_to_polyline(p, points) <= *pad,
            Area::Polylines { lines, pad } => {
                lines.iter().any(|l| distance_to_polyline(p, l) <= *pad)
            }
            Area::Band {
                center,
                radius,
                pad,
            } => (p.distance(*center) - radius).abs() <= *pad,
            Area::Frame { rect, title } => {
                title.contains(p) || edge_distance(rect, p) <= FRAME_EDGE_PX
            }
            Area::Box { rect } => rect.contains(p),
        }
    }

    /// How far `p` is from the thing itself (its lamps, its path, the inside
    /// of its hull) — "nearest wins" when areas overlap.
    pub fn distance(&self, p: Vec2) -> f64 {
        match self {
            Area::Hull { points, .. } => hull_distance(points, p),
            Area::Polyline { points, .. } => distance_to_polyline(p, points),
            Area::Polylines { lines, .. } => lines
                .iter()
                .map(|l| distance_to_polyline(p, l))
                .fold(f64::INFINITY, f64::min),
            Area::Band { center, radius, .. } => (p.distance(*center) - radius).abs(),
            Area::Frame { rect, title } => {
                if title.contains(p) {
                    0.0
                } else {
                    edge_distance(rect, p)
                }
            }
            Area::Box { rect } => {
                if rect.contains(p) {
                    0.0
                } else {
                    edge_distance(rect, p)
                }
            }
        }
    }

    /// The top of the area: where a name label goes.
    pub fn top(&self) -> Vec2 {
        match self {
            Area::Hull { points, pad } | Area::Polyline { points, pad } => {
                let top = points
                    .iter()
                    .copied()
                    .min_by(|a, b| a.y.total_cmp(&b.y))
                    .unwrap_or_default();
                Vec2::new(top.x, top.y - pad)
            }
            Area::Polylines { lines, pad } => {
                let top = lines
                    .iter()
                    .flatten()
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
            Area::Frame { title, .. } => title.min,
            Area::Box { rect } => Vec2::new((rect.min.x + rect.max.x) / 2.0, rect.min.y),
        }
    }

    /// The outline as an SVG path. A band is two circles (fill it with
    /// `evenodd`). A polyline's outline is NOT a closed shape: it is the
    /// polyline itself, and [`Area::stroke_width`] says how wide to stroke
    /// it — round caps and joins — to cover exactly the area.
    pub fn outline_path(&self) -> String {
        match self {
            Area::Hull { points, pad } => rounded_hull_path(points, *pad),
            Area::Polyline { points, .. } => polyline_path(points),
            Area::Polylines { lines, .. } => lines
                .iter()
                .map(|l| polyline_path(l))
                .collect::<Vec<_>>()
                .join(" "),
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
            Area::Frame { rect, title } => format!("{} {}", rect_path(rect), rect_path(title)),
            Area::Box { rect } => rect_path(rect),
        }
    }

    /// For an area drawn as a stroke (a path's): the stroke's width, with
    /// round caps and joins. `None` for an area whose outline is closed.
    pub fn stroke_width(&self) -> Option<f64> {
        match self {
            Area::Polyline { pad, .. } | Area::Polylines { pad, .. } => Some(2.0 * pad),
            _ => None,
        }
    }
}

fn polyline_path(points: &[Vec2]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        d.push_str(&format!(
            "{}{},{} ",
            if i == 0 { "M" } else { "L" },
            p.x,
            p.y
        ));
    }
    // A path whose points all coincide still shows as a dot.
    if points.len() == 1 {
        d.push_str("l0.01,0");
    }
    d
}

fn rect_path(r: &Rect) -> String {
    format!(
        "M{},{} H{} V{} H{} Z",
        r.min.x, r.min.y, r.max.x, r.max.y, r.min.x
    )
}

/// Distance from `p` to a rectangle's border (inside or out).
fn edge_distance(r: &Rect, p: Vec2) -> f64 {
    let a = r.min;
    let b = Vec2::new(r.max.x, r.min.y);
    let c = r.max;
    let d = Vec2::new(r.min.x, r.max.y);
    [(a, b), (b, c), (c, d), (d, a)]
        .iter()
        .map(|(u, v)| distance_to_segment(p, *u, *v))
        .fold(f64::INFINITY, f64::min)
}

impl Project {
    /// The area of one target on screen.
    pub fn area(&self, camera: &Camera, t: &Target) -> Option<Area> {
        let screen = |p: Vec2| camera.to_screen(p);
        // Components store their own fixture's coordinates.
        let origin = t
            .owner()
            .and_then(|c| self.fixture_of(c))
            .map(|n| self.origin(&n.id))
            .unwrap_or_default();
        let local = |p: Vec2| camera.to_screen(p + origin);
        match t {
            Target::Node(id) => {
                let n = self.node(id)?;
                let b = self.node_rect(id)?;
                let rect = Rect::from_corners(screen(b.min), screen(b.max));
                if !n.is_frame() {
                    return Some(Area::Box { rect });
                }
                let title = Rect::new(
                    Vec2::new(rect.min.x, rect.min.y - FRAME_TITLE_PX - 2.0),
                    Vec2::new(
                        rect.min.x + title_width(&n.name, n.pinned.is_some()),
                        rect.min.y - 2.0,
                    ),
                );
                Some(Area::Frame { rect, title })
            }
            Target::Port { output, key } => {
                let b = self.port_rect(output, key)?;
                Some(Area::Box {
                    rect: Rect::from_corners(screen(b.min), screen(b.max)),
                })
            }
            Target::Pixel { output, key, index } => {
                let b = self.pixel_rect(output, key, *index)?;
                Some(Area::Box {
                    rect: Rect::from_corners(screen(b.min), screen(b.max)),
                })
            }
            Target::Lamp { .. } => {
                let p = *self.lamps_under(t).first()?;
                Some(Area::hull(&[screen(p)], LAMP_PAD_PX))
            }
            Target::Object(o) => match &self.object(o)?.shape {
                ObjectShape::Ring { center, radius } => Some(Area::Band {
                    center: screen(*center),
                    radius: radius * camera.scale,
                    pad: RING_PAD_PX,
                }),
                ObjectShape::Polyline { points } => Some(Area::Polyline {
                    points: points.iter().map(|p| screen(*p)).collect(),
                    pad: PATH_PAD_PX,
                }),
            },
            Target::Component(id) => {
                let c = self.component(id)?;
                match &c.kind {
                    ComponentKind::Path { points, .. } => Some(Area::Polyline {
                        points: points.iter().map(|p| local(*p)).collect(),
                        pad: PATH_PAD_PX,
                    }),
                    ComponentKind::Polygon { points, .. } => Some(Area::Polyline {
                        points: points
                            .iter()
                            .chain(points.first())
                            .map(|p| local(*p))
                            .collect(),
                        pad: PATH_PAD_PX,
                    }),
                    // Each copy's own outline: a repeat's copies are apart
                    // (round a dome), and a hull round them would cover
                    // everything between.
                    ComponentKind::Repeat { .. } => Some(Area::Polylines {
                        lines: self
                            .objects_of(id)
                            .map(|o| match &o.shape {
                                ObjectShape::Polyline { points } => {
                                    points.iter().map(|p| screen(*p)).collect()
                                }
                                ObjectShape::Ring { center, radius } => {
                                    let mut ring =
                                        ring_points(screen(*center), radius * camera.scale);
                                    ring.extend(ring.first().copied());
                                    ring
                                }
                            })
                            .collect(),
                        pad: PATH_PAD_PX,
                    }),
                    ComponentKind::Circle { center, rings, .. } => {
                        let r = rings.iter().map(|r| r.radius).fold(0.0, f64::max) * camera.scale;
                        Some(Area::hull(&[local(*center)], r + CIRCLE_PAD_PX))
                    }
                    ComponentKind::Group { .. } => {
                        let mut points: Vec<Vec2> =
                            self.lamps_under(t).into_iter().map(screen).collect();
                        // Circles inside a group count by their whole disc.
                        for o in self.objects_under(id) {
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
    /// path's area, a ring's band inside its circle's disc, everything
    /// inside its group's hull, a port's row inside its card, a pixel's
    /// square inside its port's row), so a thing
    /// the cursor is outside of has nothing under the cursor inside it
    /// either. Frames are the exception: a frame's area is only its border,
    /// so look inside every one.
    fn collect_at(&self, camera: &Camera, p: Vec2, t: &Target, out: &mut Vec<Target>) {
        // Nowhere near what's drawn: nothing here, nor inside it. Every
        // area lies within its lamps' and shape's box, padded by at most
        // `GROUP_PAD_PX`, so this answers as the area would, without
        // building it — the dome's 22,000 lamps are not all asked.
        if let Some(b) = self.drawn_bounds(t) {
            let pad = (GROUP_PAD_PX + 1.0) / camera.scale;
            let w = camera.to_world(p);
            if w.x < b.min.x - pad
                || w.x > b.max.x + pad
                || w.y < b.min.y - pad
                || w.y > b.max.y + pad
            {
                return;
            }
        }
        let hit = self.area(camera, t).is_some_and(|a| a.contains(p));
        if !hit && !self.is_frame(t) {
            return;
        }
        if hit {
            out.push(t.clone());
        }
        for child in self.children(Some(t)) {
            self.collect_at(camera, p, &child, out);
        }
    }

    /// The box round what a drawn thing is made of — its lamps and shape —
    /// in world units. `None` for nodes, ports and pixels.
    fn drawn_bounds(&self, t: &Target) -> Option<Rect> {
        let placed = self.placed();
        match t {
            Target::Component(id) => placed.component_bounds(id),
            Target::Object(o) | Target::Lamp { object: o, .. } => placed.object_bounds(o),
            _ => None,
        }
    }

    /// How far `p` is from `t` on screen (`INFINITY` if it has no area).
    pub fn distance_to(&self, camera: &Camera, t: &Target, p: Vec2) -> f64 {
        self.area(camera, t)
            .map_or(f64::INFINITY, |a| a.distance(p))
    }
}

/// A frame's title tab, in pixels: wide enough for its name, and for the
/// pinned mark after it when the node is pinned (D48).
pub fn title_width(name: &str, pinned: bool) -> f64 {
    14.0 + 7.0 * name.chars().count() as f64 + if pinned { PIN_MARK_PX } else { 0.0 }
}

/// The room a pinned node's mark takes in its title tab, in pixels.
pub const PIN_MARK_PX: f64 = 12.0;

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
