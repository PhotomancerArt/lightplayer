//! Components: the things you author.
//!
//! A component has properties but no lamps of its own. It *produces*
//! objects (see `object.rs`), and it is the one that hands out their ids:
//! a circle keeps a list of ring keys, so removing ring 2 leaves ring 3
//! as `r3` and nothing that pointed at it moves (vision D10).
//!
//! Every edit to geometry lands here, even when the user makes it with an
//! object selected — a ring's lamp count lives in its circle (D11).

use crate::geom::Vec2;

/// A component's id, minted by the fixture when the component is created
/// (`line1`, `circle2`, `group1`) and never reused or rewritten.
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
    /// A straight run of lamps. Data runs `from` → `to`; reversing a line
    /// swaps its ends. Makes exactly one object.
    Line { from: Vec2, to: Vec2, lamps: u32 },
    /// Concentric rings around one centre. Makes one object per ring.
    Circle {
        center: Vec2,
        rings: Vec<Ring>,
        next_ring: u32,
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
        match self.kind {
            ComponentKind::Line { .. } => "line",
            ComponentKind::Circle { .. } => "circle",
            ComponentKind::Group { .. } => "group",
        }
    }

    /// A line is its own object: the tree shows it as ONE row, and its
    /// lamps sit directly under it (D18's 1:1 case). Decided by kind, not
    /// by the current count, so the tree never changes shape under you.
    pub fn is_single_object(&self) -> bool {
        matches!(self.kind, ComponentKind::Line { .. })
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
        match &mut self.kind {
            ComponentKind::Line { from, to, .. } => {
                *from = *from + delta;
                *to = *to + delta;
            }
            ComponentKind::Circle { center, .. } => *center = *center + delta,
            ComponentKind::Group { children } => {
                for child in children {
                    child.translate(delta);
                }
            }
        }
    }
}

impl ComponentKind {
    pub fn line(from: Vec2, to: Vec2, lamps: u32) -> Self {
        ComponentKind::Line {
            from,
            to,
            lamps: lamps.max(1),
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
