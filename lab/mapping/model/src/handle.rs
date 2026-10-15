//! Handles: the small squares you drag to reshape what's selected.
//!
//! A selected path has one on each of its points, a selected circle one on
//! each ring (a selected ring, on itself), and a selected fixture one at
//! its bottom-right corner and one on its right and bottom edges.
//!
//! A handle is its own small area, always hit before whatever is under it
//! — but only by a **drag**. A click on a handle is an ordinary click on
//! what's beneath (the end point of a path is also its first lamp, and
//! clicking it still goes into the path), so handles never get in the way
//! of selecting.
//!
//! Dragging a handle changes geometry, never a count: a path's lamps
//! respace along its new length, a ring keeps its lamps at the new radius.
//! A count is its own property, changed with − and = (D11). One drag is
//! one undo step.
//!
//! Handles sit in world units; their squares are a fixed size on screen,
//! like every area's padding.

use crate::camera::Camera;
use crate::component::{ComponentId, ComponentKind};
use crate::geom::{Rect, Vec2};
use crate::node::NodeId;
use crate::project::Project;
use crate::target::Target;

/// Half the side of a handle's square as drawn, in screen pixels…
pub const HANDLE_PX: f64 = 4.0;
/// …and as hit: a little more forgiving than what's drawn.
pub const HANDLE_HIT_PX: f64 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    /// Where it sits, in world units.
    pub at: Vec2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleKind {
    /// One point of a path, by its index in the path's points.
    PathPoint { path: ComponentId, index: usize },
    /// A ring's radius, on the ring's right-hand side.
    RingRadius { circle: ComponentId, ring: String },
    /// A fixture's size: its corner, or one of its far edges.
    FixtureSize { fixture: NodeId, edge: SizeEdge },
}

/// Which part of a fixture's box a size handle drags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeEdge {
    /// The bottom-right corner: width and height.
    Corner,
    /// The right edge: width.
    Right,
    /// The bottom edge: height.
    Bottom,
}

impl Handle {
    /// The square as drawn, on screen.
    pub fn rect(&self, cam: &Camera) -> Rect {
        square(cam.to_screen(self.at), HANDLE_PX)
    }

    /// Whether a screen point grabs it.
    pub fn hit(&self, cam: &Camera, p: Vec2) -> bool {
        square(cam.to_screen(self.at), HANDLE_HIT_PX).contains(p)
    }
}

impl HandleKind {
    /// What dragging it does, for the hint bar: "move this end".
    pub fn does(&self, p: &Project) -> String {
        match self {
            HandleKind::PathPoint { path, index } => match p.path_points(path) {
                Some(points) if *index == 0 || *index + 1 == points.len() => {
                    "move this end — the lamp count stays".into()
                }
                _ => "move this point — the lamp count stays".into(),
            },
            HandleKind::RingRadius { ring, .. } => format!("change ring {ring}'s radius"),
            HandleKind::FixtureSize { fixture, edge } => {
                let name = p.node_name(fixture);
                match edge {
                    SizeEdge::Corner => format!("resize {name} — hold ⇧ to keep it square"),
                    SizeEdge::Right => format!("change {name}'s width"),
                    SizeEdge::Bottom => format!("change {name}'s height"),
                }
            }
        }
    }
}

impl Project {
    /// The handles the selection shows, in selection order.
    pub fn handles(&self, selection: &[Target]) -> Vec<Handle> {
        let mut out = Vec::new();
        for t in selection {
            match t {
                Target::Component(id) => {
                    let origin = self.component_origin(id);
                    match self.component(id).map(|c| &c.kind) {
                        Some(ComponentKind::Path { points, .. }) => {
                            out.extend(points.iter().enumerate().map(|(index, p)| Handle {
                                kind: HandleKind::PathPoint {
                                    path: id.clone(),
                                    index,
                                },
                                at: *p + origin,
                            }))
                        }
                        Some(ComponentKind::Circle { center, rings, .. }) => {
                            out.extend(rings.iter().map(|r| Handle {
                                kind: HandleKind::RingRadius {
                                    circle: id.clone(),
                                    ring: r.key.clone(),
                                },
                                at: *center + origin + Vec2::new(r.radius, 0.0),
                            }))
                        }
                        _ => {}
                    }
                }
                Target::Object(o) => {
                    if let (Some(r), Some(ComponentKind::Circle { center, .. })) = (
                        self.ring(&o.component, &o.key),
                        self.component(&o.component).map(|c| &c.kind),
                    ) {
                        out.push(Handle {
                            kind: HandleKind::RingRadius {
                                circle: o.component.clone(),
                                ring: o.key.clone(),
                            },
                            at: *center
                                + self.component_origin(&o.component)
                                + Vec2::new(r.radius, 0.0),
                        });
                    }
                }
                Target::Node(id) if self.fixture(id).is_some() => {
                    let Some(r) = self.node_rect(id) else {
                        continue;
                    };
                    let mid = r.min.lerp(r.max, 0.5);
                    for (edge, at) in [
                        (SizeEdge::Right, Vec2::new(r.max.x, mid.y)),
                        (SizeEdge::Bottom, Vec2::new(mid.x, r.max.y)),
                        (SizeEdge::Corner, r.max),
                    ] {
                        out.push(Handle {
                            kind: HandleKind::FixtureSize {
                                fixture: id.clone(),
                                edge,
                            },
                            at,
                        });
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// A path's points, in its fixture's own coordinates.
    pub fn path_points(&self, id: &ComponentId) -> Option<&[Vec2]> {
        match &self.component(id)?.kind {
            ComponentKind::Path { points, .. } => Some(points),
            _ => None,
        }
    }

    /// Where a component's fixture puts its `(0, 0)`, in world units.
    pub fn component_origin(&self, id: &ComponentId) -> Vec2 {
        self.fixture_of(id)
            .map(|n| self.origin(&n.id))
            .unwrap_or_default()
    }

    /// Drag a handle: `base` is the project before the drag began, `from`
    /// and `to` the pointer then and now, in world units. `square` keeps a
    /// fixture square (⇧). Geometry only — no count changes.
    pub fn drag_handle(
        &mut self,
        base: &Project,
        h: &HandleKind,
        from: Vec2,
        to: Vec2,
        square: bool,
    ) {
        let delta = to - from;
        match h {
            HandleKind::PathPoint { path, index } => {
                let Some(start) = base.path_points(path).and_then(|p| p.get(*index)).copied()
                else {
                    return;
                };
                if let Some(ComponentKind::Path { points, .. }) =
                    self.component_mut(path).map(|c| &mut c.kind)
                    && let Some(p) = points.get_mut(*index)
                {
                    *p = start + delta;
                }
            }
            HandleKind::RingRadius { circle, ring } => {
                let center = match base.component(circle).map(|c| &c.kind) {
                    Some(ComponentKind::Circle { center, .. }) => {
                        *center + base.component_origin(circle)
                    }
                    _ => return,
                };
                if let Some(r) = self.ring_mut(circle, ring) {
                    r.radius = to.distance(center).max(0.5);
                }
            }
            HandleKind::FixtureSize { fixture, edge } => {
                let Some(start) = base.fixture(fixture).map(|f| f.size) else {
                    return;
                };
                let min = crate::Fixture::MIN_SIDE;
                let mut size = start;
                if matches!(edge, SizeEdge::Corner | SizeEdge::Right) {
                    size.x = (start.x + delta.x).max(min);
                }
                if matches!(edge, SizeEdge::Corner | SizeEdge::Bottom) {
                    size.y = (start.y + delta.y).max(min);
                }
                if square {
                    let side = match edge {
                        SizeEdge::Corner => size.x.max(size.y),
                        SizeEdge::Right => size.x,
                        SizeEdge::Bottom => size.y,
                    };
                    size = Vec2::new(side, side);
                }
                if let Some(f) = self.fixture_mut(fixture) {
                    f.size = size;
                }
            }
        }
    }
}

fn square(c: Vec2, half: f64) -> Rect {
    Rect::new(c - Vec2::new(half, half), c + Vec2::new(half, half))
}
