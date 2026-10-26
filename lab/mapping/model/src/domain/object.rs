//! Objects: what components render. Derived, never stored.
//!
//! Objects form a tree (`object_tree.rs`), mirroring the components the way
//! a page's nodes mirror its components: **every component renders exactly
//! one object node**, plus the objects it generates under it — a circle's
//! rings, a repeat's copies (theory "Components and objects"). Leaves hold
//! lamps; containers (a group, a circle, a repeat, a copy of a group) hold
//! none.
//!
//! An object's id is flat (theory "Ids"): its component's id, then the key
//! the component generated for it, then the repeat copies it sits in.
//! Authored nesting (groups) is never in it, so regrouping moves nothing
//! that points at an object.
//!
//! | object | id |
//! |---|---|
//! | a path (it IS its component's node) | `path1` |
//! | a circle | `circle1` |
//! | one of its rings | `circle1/r2` |
//! | a repeat's child, copy 0 | `panel` |
//! | copy 2 | `panel@2` |
//! | a ring of a repeated circle, copy 2 | `circle2/r1@2` |

use crate::domain::component::{Component, ComponentId, ComponentKind, Ring};
use crate::domain::geom::Vec2;
use crate::domain::resolve::{self, Rotation};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    /// The component that rendered it. Edits to it land there (D11).
    pub component: ComponentId,
    /// The key its component generated for it (`r2`), or empty when it is
    /// the component's own node.
    pub key: String,
    /// The repeat copy it is in, one entry per repeat it sits in, the
    /// innermost first. Trailing zeros are never kept: copy 0 carries the
    /// original's ids, so `[3]` is `@3` and `[0, 3]` (copy 0 of an inner
    /// repeat inside copy 3 of an outer one) is `@0@3`.
    pub copies: Vec<u32>,
}

impl ObjectId {
    /// The object a component renders as its own node: a path, a circle, a
    /// group, a repeat.
    pub fn of(component: &ComponentId) -> Self {
        Self::new(component, "")
    }

    /// An object the component generated under itself (a ring, `r2`).
    pub fn new(component: &ComponentId, key: impl Into<String>) -> Self {
        Self {
            component: component.clone(),
            key: key.into(),
            copies: Vec::new(),
        }
    }

    /// The same object in repeat copy `k` of a repeat round it. Innermost
    /// copies are added first, so call it from the inside out, then
    /// [`ObjectId::canonical`] once every repeat round it has had its say.
    pub fn in_copy(mut self, k: u32) -> Self {
        self.copies.push(k);
        self
    }

    /// Trailing zeros dropped: copy 0 carries the original's ids. Only
    /// once every repeat round it has added its copy, or an inner copy 3
    /// in outer copy 0 (`@3`) and an inner copy 0 in outer copy 3 (`@0@3`)
    /// would read the same.
    pub fn canonical(mut self) -> Self {
        while self.copies.last() == Some(&0) {
            self.copies.pop();
        }
        self
    }

    /// Is it its component's own node (in copy 0)?
    pub fn is_component_node(&self) -> bool {
        self.key.is_empty() && self.copies.is_empty()
    }

    /// The same object in copy 0: what its component authored.
    pub fn original(&self) -> Self {
        Self {
            copies: Vec::new(),
            ..self.clone()
        }
    }

    /// Read one back from its text (`circle2/r1@2`). `None` when it isn't
    /// one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('@');
        let head = parts.next()?;
        let copies = parts
            .map(|k| k.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()?;
        let (component, key) = match head.split_once('/') {
            Some((c, k)) => (c, k),
            None => (head, ""),
        };
        if component.is_empty() || key.contains('/') {
            return None;
        }
        let mut id = ObjectId::new(&ComponentId::new(component), key);
        id.copies = copies;
        (id.copies.last() != Some(&0)).then_some(id)
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.component)?;
        if !self.key.is_empty() {
            write!(f, "/{}", self.key)?;
        }
        for k in &self.copies {
            write!(f, "@{k}")?;
        }
        Ok(())
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

/// The leaves one component renders, in tree order: its own leaf (a path
/// is one), its generated ones (a circle's rings), or everything under it
/// (a group's children's, every copy of a repeat's child's). Positions come
/// from the product's own arithmetic (`resolve.rs`).
pub fn objects_of(component: &Component) -> Vec<Object> {
    let mut out = Vec::new();
    crate::domain::object_tree::render(component).collect_leaves(&mut out);
    out
}

/// One component's own lamps-holding object, when it makes exactly one
/// (a path, a polygon, a grid, a ring set, a filled polygon), at its own
/// id.
pub(crate) fn single_leaf(id: &ComponentId, kind: &ComponentKind) -> Option<Object> {
    let id = ObjectId::of(id);
    match kind {
        ComponentKind::Path {
            points,
            lamps,
            reversed,
            gaps,
        } => Some(Object {
            id,
            lamps: lamps_at(resolve::path_lamps(points, *lamps, *reversed, gaps)),
            shape: ObjectShape::Polyline {
                points: points.clone(),
            },
        }),
        ComponentKind::Polygon { points, lamps } => Some(Object {
            id,
            lamps: lamps_at(resolve::polygon_lamps(points, *lamps)),
            shape: ObjectShape::Polyline {
                points: points.iter().chain(points.first()).copied().collect(),
            },
        }),
        ComponentKind::Grid(grid) => Some(Object {
            id,
            lamps: lamps_at(resolve::grid_lamps(grid)),
            shape: ObjectShape::Filled {
                outline: grid.outline(),
            },
        }),
        ComponentKind::RingSet(set) => Some(Object {
            id,
            lamps: lamps_at(resolve::ring_set_lamps(set)),
            shape: ObjectShape::Rings {
                center: set.center,
                radii: set.radii(),
            },
        }),
        ComponentKind::Filled(filled) => Some(Object {
            id,
            lamps: lamps_at(resolve::filled_polygon_lamps(filled)),
            shape: ObjectShape::Filled {
                outline: filled.points.clone(),
            },
        }),
        ComponentKind::Circle { .. } | ComponentKind::Repeat { .. } | ComponentKind::Group { .. } => {
            None
        }
    }
}

/// A circle's rings, each its own leaf (`circle1/r2`). A ring of one lamp
/// is one lamp at the centre, whatever its radius (Yona, 2026-10-25: the
/// circular display's centre is its innermost ring).
pub(crate) fn ring_leaves(id: &ComponentId, center: Vec2, rings: &[Ring]) -> Vec<Object> {
    rings
        .iter()
        .map(|ring| {
            let lamps = if ring.lamps == 1 {
                vec![center]
            } else {
                resolve::ring_lamps(center, ring.radius, ring.lamps, ring.reversed)
            };
            Object {
                id: ObjectId::new(id, ring.key.clone()),
                lamps: lamps_at(lamps),
                shape: ObjectShape::Ring {
                    center,
                    radius: if ring.lamps == 1 { 0.0 } else { ring.radius },
                },
            }
        })
        .collect()
}

/// An object turned into repeat copy `k`: its lamps and shape turned, its
/// id in that copy.
pub(crate) fn turned_copy(o: &Object, turn: &Rotation, k: u32) -> Object {
    Object {
        id: o.id.clone().in_copy(k),
        lamps: o
            .lamps
            .iter()
            .map(|l| Lamp {
                pos: turn.apply(l.pos),
            })
            .collect(),
        shape: o.shape.turned(turn),
    }
}

fn lamps_at(positions: Vec<Vec2>) -> Vec<Lamp> {
    positions.into_iter().map(|pos| Lamp { pos }).collect()
}
