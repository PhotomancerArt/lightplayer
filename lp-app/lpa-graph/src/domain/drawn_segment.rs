//! What the canvas draws of a segment: its lamps, and the stroke its part
//! drew it along. Derived from a fixture's mapping, never stored.
//!
//! The lamps are `lpc_mapping::part::render`'s, the engine's own arithmetic
//! (bit for bit what the board lights); the stroke is the part's geometry
//! (a path's points, a ring's circle, a grid's box), turned into every copy
//! of the repeats round it the way the renderer turns the lamps. Both are
//! in the fixture's own units: a map2d document's coordinates, with the
//! fixture's box from `(0, 0)` (`fixture.rs`).

use lpc_mapping::Rotation2d;
use lpc_mapping::part::{
    CircleSplit, Mapping, Part, PartKind, Path, PathCommand, PathSplit, Repeat, Shape, Tree, TreeId,
};

use crate::domain::geom::Vec2;
use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;

/// One segment as the canvas draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnSegment {
    pub id: TreeRef,
    /// In data order: lamp 0 is where the data enters, as drawn.
    pub lamps: Vec<Lamp>,
    pub shape: DrawnShape,
}

/// The drawn geometry of a segment, for strokes and hit testing.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawnShape {
    /// A path's points, first to last; a polygon's, with its first point
    /// again at the end; a points part's lamps, in order.
    Polyline { points: Vec<Vec2> },
    /// One ring of a circle.
    Ring { center: Vec2, radius: f64 },
    /// A circle that is one segment for all its rings, outside first.
    Rings { center: Vec2, radii: Vec<f64> },
    /// A closed outline whose inside is the segment too: a grid's box, a
    /// filled polygon's outline.
    Filled { outline: Vec<Vec2> },
}

/// One lamp, where it sits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub pos: Vec2,
}

impl DrawnShape {
    /// Every point of it moved by `f`, and every length scaled by `scale`.
    pub fn mapped(&self, f: impl Fn(Vec2) -> Vec2, scale: f64) -> DrawnShape {
        match self {
            DrawnShape::Polyline { points } => DrawnShape::Polyline {
                points: points.iter().map(|p| f(*p)).collect(),
            },
            DrawnShape::Ring { center, radius } => DrawnShape::Ring {
                center: f(*center),
                radius: radius * scale,
            },
            DrawnShape::Rings { center, radii } => DrawnShape::Rings {
                center: f(*center),
                radii: radii.iter().map(|r| r * scale).collect(),
            },
            DrawnShape::Filled { outline } => DrawnShape::Filled {
                outline: outline.iter().map(|p| f(*p)).collect(),
            },
        }
    }
}

/// Every segment of a fixture's rendered tree, in tree order (the
/// fixture's lamp order), in the fixture's own units.
pub fn drawn_segments(fixture: &NodeId, mapping: &Mapping, tree: &Tree) -> Vec<DrawnSegment> {
    tree.segments()
        .iter()
        .map(|segment| {
            let lamps = tree
                .lamps_of(segment)
                .iter()
                .map(|p| Lamp { pos: vec2(*p) })
                .collect::<Vec<_>>();
            let shape = shape_of(mapping, &segment.id).unwrap_or_else(|| DrawnShape::Polyline {
                points: lamps.iter().map(|l| l.pos).collect(),
            });
            DrawnSegment {
                id: TreeRef::new(fixture, segment.id.clone()),
                lamps,
                shape,
            }
        })
        .collect()
}

/// What a part of this kind would draw on its own, in its fixture's units:
/// a drawing in progress, before it is a part. Its ids are `draft`'s.
pub fn drawn_draft(fixture: &NodeId, kind: PartKind) -> Vec<DrawnSegment> {
    let mut mapping = Mapping::new();
    let id = lpc_mapping::part::PartId::new("draft").expect("a valid id");
    mapping.parts.push(Part::new(id, kind));
    match lpc_mapping::part::render(&mapping) {
        Ok(tree) => drawn_segments(fixture, &mapping, &tree),
        Err(_) => Vec::new(),
    }
}

/// A segment moved from its fixture's units to world units.
pub fn in_world(mut o: DrawnSegment, frame: crate::domain::placement::Frame) -> DrawnSegment {
    for l in &mut o.lamps {
        l.pos = frame.to_world(l.pos);
    }
    o.shape = o.shape.mapped(|p| frame.to_world(p), frame.scale);
    o
}

/// The stroke of the segment `id`: its part's geometry for the key, turned
/// into the copy the id names. `None` for a part with no stroke of its own
/// (points: the caller strokes its lamps).
fn shape_of(mapping: &Mapping, id: &TreeId) -> Option<DrawnShape> {
    let part = mapping.part(&id.part)?;
    let PartKind::Shape(shape) = &part.kind else {
        return None;
    };
    let key = id.key.as_ref().map(|k| k.as_str());
    let drawn = match shape {
        Shape::Path(path) => DrawnShape::Polyline {
            points: path_points(path, key),
        },
        Shape::Polygon(polygon) => DrawnShape::Polyline {
            points: polygon
                .points
                .iter()
                .chain(polygon.points.first())
                .map(|p| vec2(*p))
                .collect(),
        },
        Shape::Circle(circle) => match (circle.split, key) {
            (CircleSplit::Rings, Some(key)) => {
                let ring = circle.rings.iter().find(|r| r.key.as_str() == key)?;
                DrawnShape::Ring {
                    center: vec2(circle.center),
                    radius: f64::from(ring.radius),
                }
            }
            _ => DrawnShape::Rings {
                center: vec2(circle.center),
                radii: circle.rings.iter().map(|r| f64::from(r.radius)).collect(),
            },
        },
        Shape::Grid(grid) => {
            let half = f64::from(grid.pitch) / 2.0;
            let (x, y) = (f64::from(grid.origin[0]), f64::from(grid.origin[1]));
            let pitch = f64::from(grid.pitch);
            let min = Vec2::new(x - half, y - half);
            let max = Vec2::new(
                x + f64::from(grid.cols.max(1) - 1) * pitch + half,
                y + f64::from(grid.rows.max(1) - 1) * pitch + half,
            );
            DrawnShape::Filled {
                outline: vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
            }
        }
        Shape::FilledPolygon(filled) => DrawnShape::Filled {
            outline: filled.points.iter().map(|p| vec2(*p)).collect(),
        },
        Shape::Points(_) => return None,
    };
    Some(turned(mapping, &part.id, &id.copies, drawn))
}

/// A path's points: all of them for one segment, or one subpath's for a
/// split path's segment `s<n>`.
fn path_points(path: &Path, key: Option<&str>) -> Vec<Vec2> {
    let all = || path.commands.iter().map(|c| vec2(c.point())).collect();
    let (PathSplit::Subpaths, Some(key)) = (path.split, key) else {
        return all();
    };
    let Some(n) = key.strip_prefix('s').and_then(|n| n.parse::<usize>().ok()) else {
        return all();
    };
    let mut subpath = 0usize;
    let mut out = Vec::new();
    for c in &path.commands {
        if matches!(c, PathCommand::Move(_)) {
            subpath += 1;
        }
        if subpath == n {
            out.push(vec2(c.point()));
        }
    }
    out
}

/// A copy-0 stroke turned into the copy `copies` names (innermost first,
/// trailing zeros dropped): each repeat round the part, innermost first,
/// turns it about its own centre, as the renderer turns lamps.
fn turned(
    mapping: &Mapping,
    part: &lpc_mapping::part::PartId,
    copies: &[u32],
    drawn: DrawnShape,
) -> DrawnShape {
    let repeats = repeats_around(&mapping.parts, part).unwrap_or_default();
    let mut drawn = drawn;
    for (i, repeat) in repeats.iter().enumerate() {
        let k = copies.get(i).copied().unwrap_or(0);
        let degrees = repeat
            .copy_degrees()
            .get(k as usize)
            .copied()
            .unwrap_or(0.0);
        let turn = Rotation2d::about(repeat.center, degrees);
        drawn = drawn.mapped(|p| vec2(turn.apply(f32s(p))), 1.0);
    }
    drawn
}

/// The repeats round a part, innermost first; `None` when it isn't there.
fn repeats_around<'a>(
    parts: &'a [Part],
    id: &lpc_mapping::part::PartId,
) -> Option<Vec<&'a Repeat>> {
    for part in parts {
        if &part.id == id {
            return Some(Vec::new());
        }
        if let Some(mut inner) = repeats_around(part.parts(), id) {
            if let PartKind::Repeat(repeat) = &part.kind {
                inner.push(repeat);
            }
            return Some(inner);
        }
    }
    None
}

/// A point of the mapping, in the lab's units.
pub fn vec2(p: [f32; 2]) -> Vec2 {
    Vec2::new(f64::from(p[0]), f64::from(p[1]))
}

/// A point of the lab, as the mapping holds it.
pub fn f32s(p: Vec2) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}
