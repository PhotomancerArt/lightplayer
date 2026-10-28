//! The geometry edits the canvas makes to parts, in the lab's units: a
//! path's points and jumpers, a circle's rings, moving a part.
//!
//! A path is SVG's moves and lines (`lpc_mapping::part::Path`): its points
//! are every command's point in order, and a move after the first makes
//! the stretch ending at it a jumper. The canvas edits points and jumpers
//! (segment `i` runs from point `i` to the next), so it reads and writes a
//! path in those terms here.

use lpc_mapping::part::{Circle, PartKind, Path, PathCommand, Ring, Shape};

use crate::domain::drawn_segment::{f32s, vec2};
use crate::domain::geom::Vec2;
use crate::domain::part_ref::PartRef;
use crate::domain::project::Project;

/// A path's points, first to last.
pub fn path_points(path: &Path) -> Vec<Vec2> {
    path.commands.iter().map(|c| vec2(c.point())).collect()
}

/// A path's jumpers: the stretches (by index: stretch `i` runs from point
/// `i` to the next) that are wire, lit by nothing.
pub fn path_gaps(path: &Path) -> Vec<u32> {
    path.commands
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, c)| c.is_move())
        .map(|(i, _)| i as u32 - 1)
        .collect()
}

/// Give a path these points and jumpers.
pub fn set_path(path: &mut Path, points: &[Vec2], gaps: &[u32]) {
    path.commands = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let jumped = i > 0 && gaps.contains(&(i as u32 - 1));
            if i == 0 || jumped {
                PathCommand::Move(f32s(*p))
            } else {
                PathCommand::Line(f32s(*p))
            }
        })
        .collect();
}

/// Move every point of a part (and the parts it holds) by `delta`.
pub fn translate(kind: &mut PartKind, delta: Vec2) {
    let by = |p: &mut [f32; 2]| *p = f32s(vec2(*p) + delta);
    match kind {
        PartKind::Shape(Shape::Path(path)) => {
            for c in &mut path.commands {
                match c {
                    PathCommand::Move(p) | PathCommand::Line(p) => by(p),
                }
            }
        }
        PartKind::Shape(Shape::Polygon(polygon)) => polygon.points.iter_mut().for_each(by),
        PartKind::Shape(Shape::Circle(circle)) => by(&mut circle.center),
        PartKind::Shape(Shape::Grid(grid)) => by(&mut grid.origin),
        PartKind::Shape(Shape::FilledPolygon(filled)) => filled.points.iter_mut().for_each(by),
        PartKind::Shape(Shape::Points(points)) => points.at.iter_mut().for_each(by),
        PartKind::Repeat(repeat) => {
            by(&mut repeat.center);
            translate(&mut repeat.part.kind, delta);
        }
        PartKind::Group { parts } => {
            for part in parts {
                translate(&mut part.kind, delta);
            }
        }
    }
}

/// Does a part of this kind have a direction of its own to reverse as
/// drawn? A path (its `reversed`: which end lamp 0 is), a circle (which
/// way round its rings run), and a repeat of either.
pub fn has_drawn_direction(kind: &PartKind) -> bool {
    match kind {
        PartKind::Shape(Shape::Path(_) | Shape::Circle(_)) => true,
        PartKind::Repeat(repeat) => has_drawn_direction(&repeat.part.kind),
        _ => false,
    }
}

/// Reverse a part's drawn direction: a path's end, a circle's way round.
pub fn reverse_drawn(kind: &mut PartKind) {
    use lpc_mapping::RingDir;
    match kind {
        PartKind::Shape(Shape::Path(path)) => path.reversed = !path.reversed,
        PartKind::Shape(Shape::Circle(circle)) => {
            circle.dir = match circle.dir {
                RingDir::Cw => RingDir::Ccw,
                RingDir::Ccw => RingDir::Cw,
            }
        }
        PartKind::Repeat(repeat) => reverse_drawn(&mut repeat.part.kind),
        _ => {}
    }
}

/// The points you drag, for a path or a polygon. A filled polygon's
/// outline is moved whole (plan M4), so it has none here.
pub fn part_points(kind: &PartKind) -> Option<Vec<Vec2>> {
    match kind {
        PartKind::Shape(Shape::Path(path)) => Some(path_points(path)),
        PartKind::Shape(Shape::Polygon(polygon)) => {
            Some(polygon.points.iter().map(|p| vec2(*p)).collect())
        }
        _ => None,
    }
}

/// Give a path or a polygon these points; a path keeps its jumpers
/// (`gaps`, when they move with an inserted or removed point).
pub fn set_part_points(kind: &mut PartKind, points: &[Vec2], gaps: Option<&[u32]>) {
    match kind {
        PartKind::Shape(Shape::Path(path)) => {
            let kept = path_gaps(path);
            set_path(path, points, gaps.unwrap_or(&kept));
        }
        PartKind::Shape(Shape::Polygon(polygon)) => {
            polygon.points = points.iter().map(|p| f32s(*p)).collect();
        }
        _ => {}
    }
}

/// The fewest points it keeps: a path two, a polygon three.
pub fn min_points(kind: &PartKind) -> usize {
    match kind {
        PartKind::Shape(Shape::Polygon(_)) => 3,
        _ => 2,
    }
}

/// A path's jumpers: its own, or the path a repeat copies.
pub fn gaps_of(kind: &PartKind) -> Option<Vec<u32>> {
    match kind {
        PartKind::Shape(Shape::Path(path)) => Some(path_gaps(path)),
        PartKind::Repeat(repeat) => gaps_of(&repeat.part.kind),
        _ => None,
    }
}

/// Give a path (or the path a repeat copies) these jumpers.
pub fn set_gaps_of(kind: &mut PartKind, gaps: &[u32]) -> bool {
    match kind {
        PartKind::Shape(Shape::Path(path)) => {
            let points = path_points(path);
            set_path(path, &points, gaps);
            true
        }
        PartKind::Repeat(repeat) => set_gaps_of(&mut repeat.part.kind, gaps),
        _ => false,
    }
}

impl Project {
    pub fn path(&self, id: &PartRef) -> Option<&Path> {
        match &self.part(id)?.kind {
            PartKind::Shape(Shape::Path(path)) => Some(path),
            _ => None,
        }
    }

    pub fn path_mut(&mut self, id: &PartRef) -> Option<&mut Path> {
        match &mut self.part_mut(id)?.kind {
            PartKind::Shape(Shape::Path(path)) => Some(path),
            _ => None,
        }
    }

    pub fn circle(&self, id: &PartRef) -> Option<&Circle> {
        match &self.part(id)?.kind {
            PartKind::Shape(Shape::Circle(circle)) => Some(circle),
            _ => None,
        }
    }

    pub fn circle_mut(&mut self, id: &PartRef) -> Option<&mut Circle> {
        match &mut self.part_mut(id)?.kind {
            PartKind::Shape(Shape::Circle(circle)) => Some(circle),
            _ => None,
        }
    }

    /// One ring of a circle, by its key.
    pub fn ring(&self, circle: &PartRef, key: &str) -> Option<&Ring> {
        self.circle(circle)?
            .rings
            .iter()
            .find(|r| r.key.as_str() == key)
    }

    pub fn ring_mut(&mut self, circle: &PartRef, key: &str) -> Option<&mut Ring> {
        self.circle_mut(circle)?
            .rings
            .iter_mut()
            .find(|r| r.key.as_str() == key)
    }

    /// The ring a tree node is, when it is one (in any copy).
    pub fn ring_of(&self, node: &crate::domain::part_ref::TreeRef) -> Option<&Ring> {
        let key = node.id.key.as_ref()?;
        self.ring(&node.part(), key.as_str())
    }
}
