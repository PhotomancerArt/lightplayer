//! Components: the things you author.
//!
//! A component has properties but no lamps of its own. It *produces*
//! objects (see `object.rs`), and it is the one that hands out their ids:
//! a circle keeps a list of ring keys, so removing ring 2 leaves ring 3
//! as `r3` and nothing that pointed at it moves (vision D10).
//!
//! Every edit to geometry lands here, even when the user makes it with an
//! object selected — a ring's lamp count lives in its circle, a copy's in
//! its repeat (D11).
//!
//! The kinds follow the product's shapes where it has them (D52): a path
//! (with gaps and a `reversed` flag), a polygon, and a repeat are the
//! product's `path`, `polygon` and `repeat`; a circle with its own rings,
//! and a group, are the lab's (fit-check §4).

use crate::domain::geom::Vec2;

/// A component's id, minted by the project when the component is created
/// (`path1`, `circle2`, `group1`) and never reused or rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentId(pub String);

impl ComponentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub id: ComponentId,
    pub kind: ComponentKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentKind {
    /// A run of lamps along a polyline of two or more points, the way a
    /// strip is laid: clicked out point by point. Data runs from the first
    /// point to the last, or from the last to the first when `reversed` —
    /// reversing never moves a point. The lamp count is its own property —
    /// moving a point respaces the lamps, it never adds or removes one.
    /// `gaps` are segments of jumper wire (by index: segment `i` runs from
    /// point `i` to the next): they carry no lamps, and the lamps keep one
    /// pitch across the rest. Makes exactly one object.
    Path {
        points: Vec<Vec2>,
        lamps: u32,
        reversed: bool,
        gaps: Vec<u32>,
    },
    /// A closed outline of three or more points (a dome's panel): lamp 0 on
    /// the first point, the rest evenly round it. Makes exactly one object.
    Polygon { points: Vec<Vec2>, lamps: u32 },
    /// Concentric rings around one centre. Makes one object per ring.
    Circle {
        center: Vec2,
        rings: Vec<Ring>,
        next_ring: u32,
    },
    /// `count` copies of one shape, turned evenly round `center`: copy `k`
    /// is the shape turned `k · 360° / count`, clockwise. Each copy is its
    /// own object — its own strand of wire — keyed by its number (`2`), the
    /// product's `/sector/2`. The copies are derived: an edit to one is an
    /// edit to the shape, so it lands on every copy (D11).
    Repeat {
        shape: Box<ComponentKind>,
        center: Vec2,
        count: u32,
    },
    /// Authored grouping: a group holds components and makes no objects of
    /// its own.
    Group { children: Vec<Component> },
}

/// One ring of a circle. `key` is minted by the circle (`r1`, `r2`, …).
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    pub key: String,
    pub radius: f64,
    pub lamps: u32,
    /// Data runs counter-clockwise instead of clockwise.
    pub reversed: bool,
}

impl Component {
    /// What the user calls this kind of thing.
    pub fn kind_word(&self) -> &'static str {
        self.kind.word()
    }

    /// A path is its own object: the tree shows it as ONE row, and its
    /// lamps sit directly under it (D18's 1:1 case). Decided by kind, not
    /// by the current count, so the tree never changes shape under you.
    pub fn is_single_object(&self) -> bool {
        matches!(
            self.kind,
            ComponentKind::Path { .. } | ComponentKind::Polygon { .. }
        )
    }

    pub fn children(&self) -> &[Component] {
        match &self.kind {
            ComponentKind::Group { children } => children,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Component>> {
        match &mut self.kind {
            ComponentKind::Group { children } => Some(children),
            _ => None,
        }
    }

    /// Move every point of this component (and any children) by `delta`.
    pub fn translate(&mut self, delta: Vec2) {
        self.kind.translate(delta);
    }
}

impl ComponentKind {
    /// A path through `points`, first to last, with `lamps` lamps.
    pub fn path(points: Vec<Vec2>, lamps: u32) -> Self {
        ComponentKind::Path {
            points,
            lamps: lamps.max(1),
            reversed: false,
            gaps: Vec::new(),
        }
    }

    /// A closed outline through `points`, with `lamps` lamps round it.
    pub fn polygon(points: Vec<Vec2>, lamps: u32) -> Self {
        ComponentKind::Polygon {
            points,
            lamps: lamps.max(1),
        }
    }

    /// `count` copies of `shape`, turned evenly round `center`.
    pub fn repeat(shape: ComponentKind, center: Vec2, count: u32) -> Self {
        ComponentKind::Repeat {
            shape: Box::new(shape),
            center,
            count: count.max(1),
        }
    }

    /// What the user calls this kind of thing.
    pub fn word(&self) -> &'static str {
        match self {
            ComponentKind::Path { .. } => "path",
            ComponentKind::Polygon { .. } => "polygon",
            ComponentKind::Circle { .. } => "circle",
            ComponentKind::Repeat { .. } => "repeat",
            ComponentKind::Group { .. } => "group",
        }
    }

    /// The points you drag, for a path or a polygon.
    pub fn points(&self) -> Option<&Vec<Vec2>> {
        match self {
            ComponentKind::Path { points, .. } | ComponentKind::Polygon { points, .. } => {
                Some(points)
            }
            _ => None,
        }
    }

    pub fn points_mut(&mut self) -> Option<&mut Vec<Vec2>> {
        match self {
            ComponentKind::Path { points, .. } | ComponentKind::Polygon { points, .. } => {
                Some(points)
            }
            _ => None,
        }
    }

    /// The fewest points it keeps: a path two, a polygon three.
    pub fn min_points(&self) -> usize {
        match self {
            ComponentKind::Polygon { .. } => 3,
            _ => 2,
        }
    }

    /// Move every point of this shape (and any children) by `delta`.
    pub fn translate(&mut self, delta: Vec2) {
        match self {
            ComponentKind::Path { points, .. } | ComponentKind::Polygon { points, .. } => {
                for p in points {
                    *p = *p + delta;
                }
            }
            ComponentKind::Circle { center, .. } => *center = *center + delta,
            ComponentKind::Repeat { shape, center, .. } => {
                *center = *center + delta;
                shape.translate(delta);
            }
            ComponentKind::Group { children } => {
                for child in children {
                    child.translate(delta);
                }
            }
        }
    }

    /// A circle with one ring per `(radius, lamps)`, keys `r1`, `r2`, ….
    pub fn circle(center: Vec2, rings: &[(f64, u32)]) -> Self {
        let rings: Vec<Ring> = rings
            .iter()
            .enumerate()
            .map(|(i, &(radius, lamps))| Ring {
                key: format!("r{}", i + 1),
                radius,
                lamps: lamps.max(1),
                reversed: false,
            })
            .collect();
        let next_ring = rings.len() as u32 + 1;
        ComponentKind::Circle {
            center,
            rings,
            next_ring,
        }
    }

    pub fn group(children: Vec<Component>) -> Self {
        ComponentKind::Group { children }
    }
}
