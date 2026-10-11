//! Objects: what components produce. Derived, never stored.
//!
//! An object has lamps. Its id is its component's id plus a key the
//! component chose (`circle1` + `r2`). A line's object key is `line`, and
//! since a line is its own object the user never sees that key.

use crate::component::{Component, ComponentId, ComponentKind};
use crate::geom::Vec2;

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
    Segment { from: Vec2, to: Vec2 },
    Ring { center: Vec2, radius: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub pos: Vec2,
}

/// The objects one component makes, in authoring order. Groups make none.
pub fn objects_of(component: &Component) -> Vec<Object> {
    match &component.kind {
        ComponentKind::Line { from, to, lamps } => {
            let n = *lamps;
            let lamps = (0..n)
                .map(|i| {
                    let t = if n == 1 {
                        0.5
                    } else {
                        i as f64 / (n - 1) as f64
                    };
                    Lamp {
                        pos: from.lerp(*to, t),
                    }
                })
                .collect();
            vec![Object {
                id: ObjectId::new(&component.id, "line"),
                lamps,
                shape: ObjectShape::Segment {
                    from: *from,
                    to: *to,
                },
            }]
        }
        ComponentKind::Circle { center, rings, .. } => rings
            .iter()
            .map(|ring| {
                let dir = if ring.reversed { -1.0 } else { 1.0 };
                // Lamp 0 sits at the top; y-down screen space makes a
                // positive angle run clockwise.
                let lamps = (0..ring.lamps)
                    .map(|i| {
                        let a = -std::f64::consts::FRAC_PI_2
                            + dir * std::f64::consts::TAU * i as f64 / ring.lamps as f64;
                        Lamp {
                            pos: *center + Vec2::new(a.cos(), a.sin()) * ring.radius,
                        }
                    })
                    .collect();
                Object {
                    id: ObjectId::new(&component.id, ring.key.clone()),
                    lamps,
                    shape: ObjectShape::Ring {
                        center: *center,
                        radius: ring.radius,
                    },
                }
            })
            .collect(),
        ComponentKind::Group { .. } => Vec::new(),
    }
}
