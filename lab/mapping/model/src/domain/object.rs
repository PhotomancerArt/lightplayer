//! Objects: what components produce. Derived, never stored.
//!
//! An object has lamps. Its id is its component's id plus a key the
//! component chose (`circle1` + `r2`, `sector` + `2`). A path's object key
//! is `path` (a polygon's `polygon`), and since a path is its own object the
//! user never sees that key.

use crate::domain::component::{Component, ComponentId, ComponentKind};
use crate::domain::geom::Vec2;
use crate::domain::resolve::{self, Rotation};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    pub component: ComponentId,
    pub key: String,
}

impl ObjectId {
    pub fn new(component: &ComponentId, key: impl Into<String>) -> Self {
        Self {
            component: component.clone(),
            key: key.into(),
        }
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.component, self.key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub id: ObjectId,
    /// In data order: lamp 0 is where the data enters.
    pub lamps: Vec<Lamp>,
    pub shape: ObjectShape,
}

/// The drawn geometry of an object, for strokes and hit testing.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectShape {
    /// A path's points, first to last; a polygon's, with its first point
    /// again at the end.
    Polyline {
        points: Vec<Vec2>,
    },
    Ring {
        center: Vec2,
        radius: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub pos: Vec2,
}

/// The objects one component makes, in authoring order. Groups make none.
/// Positions come from the product's own arithmetic (`resolve.rs`).
pub fn objects_of(component: &Component) -> Vec<Object> {
    objects_of_kind(&component.id, &component.kind)
}

/// The key of a single-object component's one object. The user never sees
/// it: a path or a polygon is its own object.
pub const PATH_KEY: &str = "path";
pub const POLYGON_KEY: &str = "polygon";

fn objects_of_kind(id: &ComponentId, kind: &ComponentKind) -> Vec<Object> {
    match kind {
        ComponentKind::Path {
            points,
            lamps,
            reversed,
            gaps,
        } => vec![Object {
            id: ObjectId::new(id, PATH_KEY),
            lamps: lamps_at(resolve::path_lamps(points, *lamps, *reversed, gaps)),
            shape: ObjectShape::Polyline {
                points: points.clone(),
            },
        }],
        ComponentKind::Polygon { points, lamps } => vec![Object {
            id: ObjectId::new(id, POLYGON_KEY),
            lamps: lamps_at(resolve::polygon_lamps(points, *lamps)),
            shape: ObjectShape::Polyline {
                points: points.iter().chain(points.first()).copied().collect(),
            },
        }],
        ComponentKind::Circle { center, rings, .. } => rings
            .iter()
            .map(|ring| Object {
                id: ObjectId::new(id, ring.key.clone()),
                lamps: lamps_at(resolve::ring_lamps(
                    *center,
                    ring.radius,
                    ring.lamps,
                    ring.reversed,
                )),
                shape: ObjectShape::Ring {
                    center: *center,
                    radius: ring.radius,
                },
            })
            .collect(),
        ComponentKind::Repeat {
            shape,
            center,
            count,
        } => {
            let inner = objects_of_kind(id, shape);
            let single = inner.len() == 1;
            let mut out = Vec::with_capacity(inner.len() * *count as usize);
            for k in 0..*count {
                let turn = Rotation::instance(*center, k, *count);
                for o in &inner {
                    let key = if single {
                        k.to_string()
                    } else {
                        format!("{k}/{}", o.id.key)
                    };
                    out.push(Object {
                        id: ObjectId::new(id, key),
                        lamps: o
                            .lamps
                            .iter()
                            .map(|l| Lamp {
                                pos: turn.apply(l.pos),
                            })
                            .collect(),
                        shape: match &o.shape {
                            ObjectShape::Polyline { points } => ObjectShape::Polyline {
                                points: points.iter().map(|p| turn.apply(*p)).collect(),
                            },
                            ObjectShape::Ring { center, radius } => ObjectShape::Ring {
                                center: turn.apply(*center),
                                radius: *radius,
                            },
                        },
                    });
                }
            }
            out
        }
        ComponentKind::Group { .. } => Vec::new(),
    }
}

fn lamps_at(positions: Vec<Vec2>) -> Vec<Lamp> {
    positions.into_iter().map(|pos| Lamp { pos }).collect()
}
