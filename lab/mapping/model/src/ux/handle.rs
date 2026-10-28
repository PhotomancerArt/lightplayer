//! Handles: the small squares you drag to reshape what's selected.
//!
//! A path has one on each of its points, a circle one on each ring (a
//! ring, on itself), and a fixture one at its bottom-right corner and one
//! on its right and bottom edges. A path's or ring's handles show while it
//! OR anything inside it is selected — a lamp included (D49) — so you can
//! walk its lamps and still reshape it.
//!
//! A handle is its own small area, hit before whatever is under it — but
//! only by a **drag**. A drag that starts on a handle moves the handle,
//! even when a selected lamp sits under it (lamps can't move on their own).
//! A click on a handle is an ordinary click on what's beneath (the end
//! point of a path is also its first lamp, and clicking it still goes into
//! the path), so handles never get in the way of selecting. Handles are
//! never assumed to sit on lamps: Béziers are coming, whose control points
//! sit on none.
//!
//! Points come and go too: double-click a shaped path's segment (or
//! right-click it, "Add a point here") to add one there, and ⌥-click a
//! point's handle (or right-click it, "Remove this point") to take it
//! away. A path keeps at least two.
//!
//! Dragging a handle changes geometry, never a count: a path's lamps
//! respace along its new length, a ring keeps its lamps at the new radius.
//! A count is its own property, changed with − and = (D11). One drag is
//! one undo step.
//!
//! Handles sit in world units; their squares are a fixed size on screen,
//! like every area's padding.

use crate::domain::component::{ComponentId, ComponentKind};
use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::NodeId;
use crate::domain::placement::Frame;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::camera::Camera;

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
    /// What dragging it does, for the hint bar.
    pub fn verb(&self) -> &'static str {
        match self {
            HandleKind::PathPoint { .. } => "move point",
            HandleKind::RingRadius { .. } => "radius",
            HandleKind::FixtureSize { edge, .. } => match edge {
                SizeEdge::Corner => "resize",
                SizeEdge::Right => "width",
                SizeEdge::Bottom => "height",
            },
        }
    }

    /// The bar's lead while it is dragged.
    pub fn lead(&self) -> &'static str {
        match self {
            HandleKind::PathPoint { .. } => "Moving a point",
            HandleKind::RingRadius { .. } => "Changing a radius",
            HandleKind::FixtureSize { .. } => "Resizing",
        }
    }
}

impl Project {
    /// The handles the selection shows, in selection order: a selected
    /// thing's own, and those of the path or ring a selected lamp is on.
    pub fn handles(&self, selection: &[Target]) -> Vec<Handle> {
        let mut out: Vec<Handle> = Vec::new();
        for t in selection {
            for h in self.handles_of(t) {
                if !out.iter().any(|o| o.kind == h.kind) {
                    out.push(h);
                }
            }
        }
        out
    }

    fn handles_of(&self, t: &Target) -> Vec<Handle> {
        let mut out = Vec::new();
        match t {
            Target::Component(id) => {
                let frame = self.component_frame(id);
                match self.component(id).map(|c| &c.kind) {
                    Some(
                        ComponentKind::Path { points, .. } | ComponentKind::Polygon { points, .. },
                    ) => out.extend(points.iter().enumerate().map(|(index, p)| Handle {
                        kind: HandleKind::PathPoint {
                            path: id.clone(),
                            index,
                        },
                        at: frame.to_world(*p),
                    })),
                    Some(ComponentKind::Circle { center, rings, .. }) => {
                        out.extend(rings.iter().map(|r| Handle {
                            kind: HandleKind::RingRadius {
                                circle: id.clone(),
                                ring: r.key.clone(),
                            },
                            at: frame.to_world(*center + Vec2::new(r.radius, 0.0)),
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
                        at: self
                            .component_frame(&o.component)
                            .to_world(*center + Vec2::new(r.radius, 0.0)),
                    });
                }
            }
            // A lamp shows the handles of the path or ring it is on.
            Target::Lamp { object, .. } => return self.handles_of(&self.object_target(object)),
            Target::Node(id) if self.fixture(id).is_some_and(|f| f.unsupported.is_none()) => {
                let Some(r) = self.node_rect(id) else {
                    return out;
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
        out
    }

    /// Add a point to a path on its segment `segment` (from point `segment`
    /// to the next), at `at` in world units. The path keeps its lamps.
    pub fn insert_path_point(&mut self, path: &ComponentId, segment: usize, at: Vec2) -> bool {
        let frame = self.component_frame(path);
        let inserted = match self.component_mut(path).and_then(|c| c.kind.points_mut()) {
            Some(points) if segment + 1 < points.len() => {
                points.insert(segment + 1, frame.to_local(at));
                true
            }
            _ => false,
        };
        // A jumper split in two is two jumpers; the segments after it move
        // up one.
        if inserted && let Some(gaps) = self.path_gaps_mut(path) {
            let s = segment as u32;
            let was = gaps.contains(&s);
            for g in gaps.iter_mut().filter(|g| **g > s) {
                *g += 1;
            }
            if was {
                gaps.push(s + 1);
                gaps.sort_unstable();
            }
        }
        inserted
    }

    /// Make a path's segment `segment` a jumper (wire, no lamps) or lit
    /// again. Returns whether it is a jumper now; `None` when it isn't a
    /// path's segment. A path keeps at least one lit segment.
    pub fn toggle_path_jumper(&mut self, path: &ComponentId, segment: usize) -> Option<bool> {
        let segments = self.path_points(path)?.len().saturating_sub(1);
        if segment >= segments {
            return None;
        }
        let gaps = self.path_gaps_mut(path)?;
        let s = segment as u32;
        if let Some(i) = gaps.iter().position(|g| *g == s) {
            gaps.remove(i);
            return Some(false);
        }
        if gaps.len() + 1 >= segments {
            return None;
        }
        gaps.push(s);
        gaps.sort_unstable();
        Some(true)
    }

    /// A path's jumpers (by segment), when it is a path: its own, or the
    /// path a repeat copies.
    pub fn path_jumpers(&self, path: &ComponentId) -> &[u32] {
        fn of(kind: &ComponentKind) -> Option<&Vec<u32>> {
            match kind {
                ComponentKind::Path { gaps, .. } => Some(gaps),
                ComponentKind::Repeat { child, .. } => of(&child.kind),
                _ => None,
            }
        }
        self.component(path)
            .and_then(|c| of(&c.kind))
            .map_or(&[], |g| g.as_slice())
    }

    fn path_gaps_mut(&mut self, path: &ComponentId) -> Option<&mut Vec<u32>> {
        fn of(kind: &mut ComponentKind) -> Option<&mut Vec<u32>> {
            match kind {
                ComponentKind::Path { gaps, .. } => Some(gaps),
                ComponentKind::Repeat { child, .. } => of(&mut child.kind),
                _ => None,
            }
        }
        self.component_mut(path).and_then(|c| of(&mut c.kind))
    }

    /// Take one point away from a path (or a polygon). Refused (false)
    /// below two points (a polygon: three).
    pub fn remove_path_point(&mut self, path: &ComponentId, index: usize) -> bool {
        let Some(kind) = self.component_mut(path).map(|c| &mut c.kind) else {
            return false;
        };
        let min = kind.min_points();
        let removed = match kind.points_mut() {
            Some(points) if points.len() > min && index < points.len() => {
                points.remove(index);
                true
            }
            _ => false,
        };
        // The two segments either side of the point become one: a jumper
        // when both were, lit otherwise (an end point takes its one segment
        // with it); the segments after it move down one.
        if removed && let Some(gaps) = self.path_gaps_mut(path) {
            let i = index as u32;
            let merged_jumper = i > 0 && gaps.contains(&(i - 1)) && gaps.contains(&i);
            gaps.retain(|g| *g != i && !(i > 0 && *g == i - 1));
            for g in gaps.iter_mut().filter(|g| **g > i) {
                *g -= 1;
            }
            if merged_jumper {
                gaps.push(i - 1);
            }
            gaps.sort_unstable();
        }
        removed
    }

    /// The segment of a path nearest `p` (screen), within its area, and the
    /// point on it nearest `p` (world): where a new point would go.
    pub fn segment_at(&self, cam: &Camera, path: &ComponentId, p: Vec2) -> Option<(usize, Vec2)> {
        let frame = self.component_frame(path);
        let points = self.path_points(path)?;
        let world = cam.to_world(p);
        points
            .windows(2)
            .enumerate()
            .map(|(i, w)| {
                let (a, b) = (frame.to_world(w[0]), frame.to_world(w[1]));
                let ab = b - a;
                let len2 = ab.x * ab.x + ab.y * ab.y;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((world.x - a.x) * ab.x + (world.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                };
                (i, a.lerp(b, t))
            })
            .map(|(i, q)| (i, q, cam.to_screen(q).distance(p)))
            .filter(|(_, _, d)| *d <= crate::ux::area::PATH_PAD_PX)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(i, q, _)| (i, q))
    }

    /// A path's points, in its fixture's own coordinates.
    pub fn path_points(&self, id: &ComponentId) -> Option<&[Vec2]> {
        self.component(id)?.kind.points().map(|p| p.as_slice())
    }

    /// Where a component's fixture puts its own units, in world units.
    pub fn component_frame(&self, id: &ComponentId) -> Frame {
        self.fixture_of(id)
            .map(|n| self.frame(&n.id))
            .unwrap_or(Frame {
                origin: Vec2::default(),
                scale: 1.0,
            })
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
        // The drag in the handle's fixture's own units.
        let frame = match h {
            HandleKind::PathPoint { path, .. } => base.component_frame(path),
            HandleKind::RingRadius { circle, .. } => base.component_frame(circle),
            HandleKind::FixtureSize { fixture, .. } => base.frame(fixture),
        };
        let delta = frame.delta_local(to - from);
        match h {
            HandleKind::PathPoint { path, index } => {
                let Some(start) = base.path_points(path).and_then(|p| p.get(*index)).copied()
                else {
                    return;
                };
                if let Some(points) = self.component_mut(path).and_then(|c| c.kind.points_mut())
                    && let Some(p) = points.get_mut(*index)
                {
                    *p = start + delta;
                }
            }
            HandleKind::RingRadius { circle, ring } => {
                let center = match base.component(circle).map(|c| &c.kind) {
                    Some(ComponentKind::Circle { center, .. }) => *center,
                    _ => return,
                };
                if let Some(r) = self.ring_mut(circle, ring) {
                    r.radius = frame.to_local(to).distance(center).max(0.5);
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
