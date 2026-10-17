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
    /// A ring set's rings, outer first: one object, several circles.
    Rings {
        center: Vec2,
        radii: Vec<f64>,
    },
    /// A closed outline whose inside is the object too: a grid's box, a
    /// filled polygon's outline.
    Filled {
        outline: Vec<Vec2>,
    },
}

impl ObjectShape {
    /// The shape turned by `turn`: a repeat's copy.
    pub fn turned(&self, turn: &Rotation) -> ObjectShape {
        match self {
            ObjectShape::Polyline { points } => ObjectShape::Polyline {
                points: points.iter().map(|p| turn.apply(*p)).collect(),
            },
            ObjectShape::Ring { center, radius } => ObjectShape::Ring {
                center: turn.apply(*center),
                radius: *radius,
            },
            ObjectShape::Rings { center, radii } => ObjectShape::Rings {
                center: turn.apply(*center),
                radii: radii.clone(),
            },
            ObjectShape::Filled { outline } => ObjectShape::Filled {
                outline: outline.iter().map(|p| turn.apply(*p)).collect(),
            },
        }
    }
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
pub const GRID_KEY: &str = "grid";
pub const RINGS_KEY: &str = "rings";
pub const FILLED_KEY: &str = "filled";

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
                        shape: o.shape.turned(&turn),
                    });
                }
            }
            out
        }
        ComponentKind::Group { .. } => Vec::new(),
        ComponentKind::Grid(grid) => vec![Object {
            id: ObjectId::new(id, GRID_KEY),
            lamps: lamps_at(resolve::grid_lamps(grid)),
            shape: ObjectShape::Filled {
                outline: grid.outline(),
            },
        }],
        ComponentKind::RingSet(set) => vec![Object {
            id: ObjectId::new(id, RINGS_KEY),
            lamps: lamps_at(resolve::ring_set_lamps(set)),
            shape: ObjectShape::Rings {
                center: set.center,
                radii: set.radii(),
            },
        }],
        ComponentKind::Filled(filled) => vec![Object {
            id: ObjectId::new(id, FILLED_KEY),
            lamps: lamps_at(resolve::filled_polygon_lamps(filled)),
            shape: ObjectShape::Filled {
                outline: filled.points.clone(),
            },
        }],
    }
}

fn lamps_at(positions: Vec<Vec2>) -> Vec<Lamp> {
    positions.into_iter().map(|pos| Lamp { pos }).collect()
}
