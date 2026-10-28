//! Render a mapping's parts into the tree (`tree.rs`), with every segment's
//! lamps.
//!
//! **Geometry is today's arithmetic, never a copy.** Each shape's lamps come
//! from today's resolver itself: the shape as a one-object document through
//! [`crate::resolve_into`], the call the device makes today. So a part's
//! segments carry the lamps today's [`crate::resolve`] gives the same
//! geometry, bit for bit. A path goes through today's path (its moves as
//! jumpers); a circle's ring k through a single-ring ring set, except a ring
//! of radius 0, which is one lamp at the centre; points are where they are
//! listed; a repeat turns every lamp of copy k by today's
//! [`crate::Rotation2d`] in today's order, copy 0 included.
//!
//! Only the resolver's public entry points are called, never its private
//! helpers: a new caller of a private function changes how the compiler
//! inlines it into today's resolver, and the firmware image must stay
//! byte-identical while nothing in it uses this module.

use alloc::string::ToString;
use alloc::vec::Vec;

use crate::map2d_doc::{Map2dDoc, Map2dObject, Map2dShape};
use crate::map2d_error::Map2dError;
use crate::map2d_resolve::{Rotation2d, filled_polygon_cells, resolve_into};
use crate::part::circle::{Circle, Ring};
use crate::part::circle_split::CircleSplit;
use crate::part::lamp_ref::LampRef;
use crate::part::mapping::Mapping;
use crate::part::part::Part;
use crate::part::part_id::PartId;
use crate::part::part_kind::PartKind;
use crate::part::path::Path;
use crate::part::path_split::PathSplit;
use crate::part::render_error::RenderError;
use crate::part::repeat::Repeat;
use crate::part::segment::Segment;
use crate::part::segment_key::SegmentKey;
use crate::part::shape::Shape;
use crate::part::subpath_runs::subpath_runs;
use crate::part::tree::Tree;
use crate::part::tree_id::TreeId;
use crate::part::tree_node::TreeNode;
use crate::sorted_set::insert_sorted;

/// Render a mapping: its parts as one tree, its segments' lamps in one
/// buffer of exactly the size they need.
pub fn render(mapping: &Mapping) -> Result<Tree, RenderError> {
    render_as(mapping, Lamps::Placed)
}

/// [`render`]'s tree without the lamps' positions: the same nodes and
/// segments, every lamp at `[0, 0]`. For a reader of the structure alone,
/// the patcher: a patch renders its mapping only to walk the tree, and the
/// positions — soft-float `sinf`/`cosf` per lamp of a circle on the C6,
/// ~1.0 M cycles of `projects/test/basic`'s first patch — were dropped
/// unread.
///
/// It refuses what `render` refuses, except a shape whose numbers today's
/// resolver would refuse (a path of no length, say): that is only found by
/// placing its lamps. Lay out a mapping that has rendered.
pub fn render_layout(mapping: &Mapping) -> Result<Tree, RenderError> {
    render_as(mapping, Lamps::Counted)
}

/// Whether a render places its lamps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lamps {
    /// Every lamp where today's resolver puts it.
    Placed,
    /// Every lamp at `[0, 0]`: the tree's structure alone ([`render_layout`]).
    Counted,
}

fn render_as(mapping: &Mapping, mode: Lamps) -> Result<Tree, RenderError> {
    let mut seen = Vec::new();
    for part in mapping.all_parts() {
        if !insert_sorted(&mut seen, part.id.as_str()) {
            return Err(RenderError::DuplicateId(part.id.clone()));
        }
    }

    let total: u64 = mapping.parts.iter().map(lamp_count).sum();
    let mut positions = Vec::new();
    // The one big ask a mapping makes, tried first: a lamp count the heap
    // can't hold refuses the mapping instead of aborting.
    let fits = usize::try_from(total)
        .ok()
        .is_some_and(|total| positions.try_reserve_exact(total).is_ok());
    if !fits {
        let bytes = total.saturating_mul(core::mem::size_of::<[f32; 2]>() as u64);
        return Err(RenderError::TooBigForMemory {
            lamps: u32::try_from(total).unwrap_or(u32::MAX),
            bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
        });
    }

    // The nodes and segments at the size they will be, so neither grows by
    // doubling (65 nodes took a 128-node buffer, 8,192 B on RV32). A count
    // the heap refuses here is left to the pushes, as before.
    let (node_total, segment_total) = mapping
        .parts
        .iter()
        .map(node_and_segment_count)
        .fold((0u64, 0u64), |(n, s), (dn, ds)| {
            (n.saturating_add(dn), s.saturating_add(ds))
        });
    let mut nodes = Vec::new();
    let mut segments = Vec::new();
    if let (Ok(n), Ok(s)) = (usize::try_from(node_total), usize::try_from(segment_total)) {
        let _ = nodes.try_reserve_exact(n);
        let _ = segments.try_reserve_exact(s);
    }

    let mut tree = Tree {
        nodes,
        segments,
        positions,
    };
    for part in &mapping.parts {
        render_part(&mut tree, part, None, &mapping.lamp, mode)?;
    }
    debug_assert_eq!(
        tree.positions.len() as u64,
        total,
        "lamp_count drifted from the render: the exact-size reserve is broken"
    );
    debug_assert!(
        tree.nodes.len() as u64 <= node_total && tree.segments.len() as u64 <= segment_total,
        "node_and_segment_count fell short of the render: the reserve is too small"
    );
    // Every repeat round each node has added its copy: now drop the
    // trailing zeros.
    for node in &mut tree.nodes {
        node.id.drop_trailing_zeros();
    }
    for segment in &mut tree.segments {
        segment.id.drop_trailing_zeros();
    }
    Ok(tree)
}

/// One part's node, and everything under it.
fn render_part(
    tree: &mut Tree,
    part: &Part,
    parent: Option<usize>,
    lamp_above: &LampRef,
    mode: Lamps,
) -> Result<(), RenderError> {
    let lamp = part.lamp.over(lamp_above);
    let me = tree.nodes.len();
    let first_segment = tree.segments.len();
    tree.nodes.push(TreeNode {
        id: TreeId::of(&part.id),
        parent,
        end: me + 1,
        segment: None,
        segments: first_segment..first_segment,
    });
    match &part.kind {
        PartKind::Shape(shape) => render_shape(tree, &part.id, shape, me, &lamp, mode)?,
        PartKind::Group { parts } => {
            for child in parts {
                render_part(tree, child, Some(me), &lamp, mode)?;
            }
        }
        PartKind::Repeat(repeat) => render_repeat(tree, &part.id, repeat, me, &lamp, mode)?,
    }
    tree.nodes[me].end = tree.nodes.len();
    tree.nodes[me].segments = first_segment..tree.segments.len();
    Ok(())
}

fn render_shape(
    tree: &mut Tree,
    id: &PartId,
    shape: &Shape,
    me: usize,
    lamp: &LampRef,
    mode: Lamps,
) -> Result<(), RenderError> {
    let today = match shape {
        Shape::Path(path) => return render_path(tree, id, path, me, lamp, mode),
        Shape::Circle(circle) => return render_circle(tree, id, circle, me, lamp, mode),
        _ if mode == Lamps::Counted => {
            let segment = push_segment(tree, TreeId::of(id), &counted(shape_count(shape)), lamp);
            tree.nodes[me].segment = Some(segment);
            return Ok(());
        }
        Shape::Points(points) => {
            let segment = push_segment(tree, TreeId::of(id), &points.at, lamp);
            tree.nodes[me].segment = Some(segment);
            return Ok(());
        }
        Shape::Polygon(polygon) => Map2dShape::Polygon(polygon.clone()),
        Shape::Grid(grid) => Map2dShape::Grid(grid.clone()),
        Shape::FilledPolygon(filled) => Map2dShape::FilledPolygon(filled.clone()),
    };
    let lamps = resolve_today(id, today)?;
    let segment = push_segment(tree, TreeId::of(id), &lamps, lamp);
    tree.nodes[me].segment = Some(segment);
    Ok(())
}

/// One shape's lamps, from today's resolver: the shape as the one object of
/// a document, through [`resolve_into`].
fn resolve_today(id: &PartId, shape: Map2dShape) -> Result<Vec<[f32; 2]>, RenderError> {
    let mut doc = Map2dDoc::new();
    doc.objects.push(Map2dObject {
        name: id.to_string(),
        id: None,
        stride: None,
        shape,
    });
    let mut lamps = Vec::new();
    let mut strands = Vec::new();
    match resolve_into(&doc, &mut lamps, &mut strands) {
        Ok(()) => Ok(lamps),
        Err(Map2dError::TooBigForMemory { lamps, bytes }) => {
            Err(RenderError::TooBigForMemory { lamps, bytes })
        }
        Err(error) => Err(RenderError::from_shape(id, error)),
    }
}

/// A path: today's path's lamps, as one segment at the path's own id, or
/// cut into one segment per subpath (`s1`, `s2`…) under it.
fn render_path(
    tree: &mut Tree,
    id: &PartId,
    path: &Path,
    me: usize,
    lamp: &LampRef,
    mode: Lamps,
) -> Result<(), RenderError> {
    let today = path
        .to_path_shape()
        .map_err(|reason| RenderError::InvalidPart {
            part: id.clone(),
            reason: reason.to_string(),
        })?;
    let lamps = match mode {
        Lamps::Placed => resolve_today(id, Map2dShape::Path(today.clone()))?,
        Lamps::Counted => counted(u64::from(path.count)),
    };
    match path.split {
        PathSplit::One => {
            let segment = push_segment(tree, TreeId::of(id), &lamps, lamp);
            tree.nodes[me].segment = Some(segment);
        }
        PathSplit::Subpaths => {
            for run in subpath_runs(path, &today) {
                let range = run.start as usize..(run.start + run.count) as usize;
                push_leaf(
                    tree,
                    TreeId::generated(id, SegmentKey::subpath(run.drawn)),
                    me,
                    &lamps[range],
                    lamp,
                );
            }
        }
    }
    Ok(())
}

/// A circle: one segment per ring, in the circle's ring order, or one
/// segment of every ring's lamps in that order. A ring of radius 0 is one
/// lamp at the centre.
fn render_circle(
    tree: &mut Tree,
    id: &PartId,
    circle: &Circle,
    me: usize,
    lamp: &LampRef,
    mode: Lamps,
) -> Result<(), RenderError> {
    let first = tree.positions.len() as u32;
    for ring in circle.rings_in_order() {
        let lamps = ring_lamps(id, circle, ring, mode)?;
        match circle.split {
            CircleSplit::Rings => push_leaf(
                tree,
                TreeId::generated(id, ring.key.clone()),
                me,
                &lamps,
                lamp,
            ),
            CircleSplit::One => tree.positions.extend_from_slice(&lamps),
        }
    }
    if circle.split == CircleSplit::One {
        tree.segments.push(Segment {
            id: TreeId::of(id),
            lamps: first..tree.positions.len() as u32,
            lamp: lamp.clone(),
        });
        tree.nodes[me].segment = Some(tree.segments.len() - 1);
    }
    Ok(())
}

/// One ring's lamps: today's single ring, or the centre.
fn ring_lamps(
    id: &PartId,
    circle: &Circle,
    ring: &Ring,
    mode: Lamps,
) -> Result<Vec<[f32; 2]>, RenderError> {
    if !ring.is_centre() {
        return match mode {
            Lamps::Placed => resolve_today(id, Map2dShape::Ring(circle.ring_shape(ring))),
            Lamps::Counted => Ok(counted(u64::from(ring.count))),
        };
    }
    if ring.count != 1 {
        return Err(RenderError::InvalidPart {
            part: id.clone(),
            reason: alloc::format!(
                "ring {} is the centre (radius 0) and holds one lamp, not {}",
                ring.key,
                ring.count
            ),
        });
    }
    Ok(match mode {
        Lamps::Placed => alloc::vec![circle.center],
        Lamps::Counted => counted(1),
    })
}

/// A repeat: copy 0 is its part's own subtree; copy k the same subtree with
/// every id in copy k and every lamp turned, in today's order (copy 0's
/// lamps first, then copy 1's…), copy 0 turned too.
fn render_repeat(
    tree: &mut Tree,
    id: &PartId,
    repeat: &Repeat,
    me: usize,
    lamp: &LampRef,
    mode: Lamps,
) -> Result<(), RenderError> {
    if repeat.count == 0 {
        return Err(RenderError::InvalidPart {
            part: id.clone(),
            reason: "repeat count must be at least 1".to_string(),
        });
    }
    let first_lamp = tree.positions.len();
    let first_node = tree.nodes.len();
    let first_segment = tree.segments.len();
    render_part(tree, &repeat.part, Some(me), lamp, mode)?;
    let lamps = tree.positions.len() - first_lamp;
    let nodes = tree.nodes.len() - first_node;
    let segments = tree.segments.len() - first_segment;
    let degrees = repeat.copy_degrees();

    // Copies 1.. from copy 0 as rendered (not yet turned, not yet in a copy).
    for (k, &turn) in degrees.iter().enumerate().skip(1) {
        if mode == Lamps::Placed {
            let rotation = Rotation2d::about(repeat.center, turn);
            for i in 0..lamps {
                let position = tree.positions[first_lamp + i];
                tree.positions.push(rotation.apply(position));
            }
        } else {
            tree.positions
                .resize(tree.positions.len() + lamps, [0.0; 2]);
        }
        let node_shift = k * nodes;
        let segment_shift = k * segments;
        let lamp_shift = (k * lamps) as u32;
        for i in 0..nodes {
            let node = &tree.nodes[first_node + i];
            let copy = TreeNode {
                id: node.id.clone().in_copy(k as u32),
                parent: if i == 0 {
                    Some(me)
                } else {
                    node.parent.map(|p| p + node_shift)
                },
                end: node.end + node_shift,
                segment: node.segment.map(|s| s + segment_shift),
                segments: node.segments.start + segment_shift..node.segments.end + segment_shift,
            };
            tree.nodes.push(copy);
        }
        for i in 0..segments {
            let segment = &tree.segments[first_segment + i];
            let copy = Segment {
                id: segment.id.clone().in_copy(k as u32),
                lamps: segment.lamps.start + lamp_shift..segment.lamps.end + lamp_shift,
                lamp: segment.lamp.clone(),
            };
            tree.segments.push(copy);
        }
    }

    // Copy 0, last: turned by its own angle, its ids in copy 0.
    if mode == Lamps::Placed {
        let rotation = Rotation2d::about(repeat.center, degrees[0]);
        for position in &mut tree.positions[first_lamp..first_lamp + lamps] {
            *position = rotation.apply(*position);
        }
    }
    for node in &mut tree.nodes[first_node..first_node + nodes] {
        node.id.copies.push(0);
    }
    for segment in &mut tree.segments[first_segment..first_segment + segments] {
        segment.id.copies.push(0);
    }
    Ok(())
}

/// `n` lamps at `[0, 0]`: a shape's count, unplaced ([`Lamps::Counted`]).
fn counted(n: u64) -> Vec<[f32; 2]> {
    alloc::vec![[0.0; 2]; usize::try_from(n).unwrap_or(usize::MAX)]
}

/// A generated segment: its own node under `parent`.
fn push_leaf(tree: &mut Tree, id: TreeId, parent: usize, lamps: &[[f32; 2]], lamp: &LampRef) {
    let index = tree.nodes.len();
    let segment = push_segment(tree, id.clone(), lamps, lamp);
    tree.nodes.push(TreeNode {
        id,
        parent: Some(parent),
        end: index + 1,
        segment: Some(segment),
        segments: segment..segment + 1,
    });
}

fn push_segment(tree: &mut Tree, id: TreeId, lamps: &[[f32; 2]], lamp: &LampRef) -> usize {
    let start = tree.positions.len() as u32;
    tree.positions.extend_from_slice(lamps);
    tree.segments.push(Segment {
        id,
        lamps: start..tree.positions.len() as u32,
        lamp: lamp.clone(),
    });
    tree.segments.len() - 1
}

/// How many lamps a part renders: what the render will push, exactly.
fn lamp_count(part: &Part) -> u64 {
    match &part.kind {
        PartKind::Shape(shape) => shape_count(shape),
        PartKind::Group { parts } => parts.iter().map(lamp_count).sum(),
        PartKind::Repeat(repeat) => u64::from(repeat.count) * lamp_count(&repeat.part),
    }
}

/// How many nodes and segments a part renders: exactly what the render
/// pushes, except a path split by subpaths, counted at its most (one per
/// subpath, a subpath with no lamps making none).
fn node_and_segment_count(part: &Part) -> (u64, u64) {
    match &part.kind {
        PartKind::Shape(shape) => {
            // Leaves under the part's own node, or the node is the segment.
            let leaves = match shape {
                Shape::Path(path) if path.split == PathSplit::Subpaths => subpath_most(path),
                Shape::Circle(circle) if circle.split == CircleSplit::Rings => {
                    circle.rings.len() as u64
                }
                _ => return (1, 1),
            };
            (1 + leaves, leaves)
        }
        PartKind::Group { parts } => parts
            .iter()
            .map(node_and_segment_count)
            .fold((1u64, 0u64), |(n, s), (dn, ds)| {
                (n.saturating_add(dn), s.saturating_add(ds))
            }),
        PartKind::Repeat(repeat) => {
            let (n, s) = node_and_segment_count(&repeat.part);
            let count = u64::from(repeat.count);
            (
                n.saturating_mul(count).saturating_add(1),
                s.saturating_mul(count),
            )
        }
    }
}

/// The most segments a path split by subpaths renders: one per subpath (a
/// move after the first command starts another), never more than its lamps.
fn subpath_most(path: &Path) -> u64 {
    if path.count == 0 || path.commands.len() < 2 {
        return 0;
    }
    let jumpers = path.commands[1..].iter().filter(|c| c.is_move()).count() as u64;
    (jumpers + 1).min(u64::from(path.count))
}

fn shape_count(shape: &Shape) -> u64 {
    match shape {
        Shape::Path(path) => u64::from(path.count),
        Shape::Polygon(polygon) => u64::from(polygon.count),
        Shape::Circle(circle) => circle.rings.iter().map(|r| u64::from(r.count)).sum(),
        Shape::Points(points) => points.at.len() as u64,
        Shape::Grid(grid) => u64::from(grid.cols) * u64::from(grid.rows),
        Shape::FilledPolygon(filled) => filled_polygon_cells(filled).len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::path_command::PathCommand;
    use crate::part::points::Points;
    use alloc::vec;

    /// The tree's nodes and segments are allocated at the size they are,
    /// not grown by doubling: a repeat of 64 spokes, and a group of a circle
    /// split into rings, a split path and points.
    #[test]
    fn the_nodes_and_segments_are_allocated_at_their_size() {
        let mut m = Mapping::new();
        let spoke = part("path1", straight(8));
        m.parts.push(part(
            "spoke",
            PartKind::Repeat(Repeat::new(spoke, [50.0, 50.0], 64)),
        ));
        let mut split = Path::of(
            vec![
                PathCommand::Move([0.0, 0.0]),
                PathCommand::Line([10.0, 0.0]),
                PathCommand::Move([20.0, 0.0]),
                PathCommand::Line([30.0, 0.0]),
            ],
            6,
        );
        split.split = PathSplit::Subpaths;
        m.parts.push(part(
            "group1",
            PartKind::Group {
                parts: vec![
                    part(
                        "circle1",
                        PartKind::Shape(Shape::Circle(Circle::new(
                            [50.0, 50.0],
                            &[(40.0, 24), (20.0, 12), (0.0, 1)],
                        ))),
                    ),
                    part("path2", PartKind::Shape(Shape::Path(split))),
                    part(
                        "points1",
                        PartKind::Shape(Shape::Points(Points {
                            at: vec![[1.0, 1.0], [2.0, 2.0]],
                        })),
                    ),
                ],
            },
        ));
        let tree = render(&m).unwrap();
        // The repeat and its 64 copies; the group, the circle and its 3
        // rings, the path and its 2 subpaths, the points.
        assert_eq!(tree.nodes.len(), 65 + 1 + 4 + 3 + 1);
        assert_eq!(tree.nodes.capacity(), tree.nodes.len());
        assert_eq!(tree.segments.len(), 64 + 3 + 2 + 1);
        assert_eq!(tree.segments.capacity(), tree.segments.len());
    }

    fn part(id: &str, kind: PartKind) -> Part {
        Part::new(PartId::new(id).unwrap(), kind)
    }

    fn straight(count: u32) -> PartKind {
        PartKind::Shape(Shape::Path(Path::through(
            &[[50.0, 40.0], [50.0, 4.0]],
            count,
        )))
    }
}
