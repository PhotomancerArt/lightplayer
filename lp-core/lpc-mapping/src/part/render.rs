//! Render a mapping's parts into the tree (`tree.rs`), with every segment's
//! lamps.
//!
//! **Geometry is today's arithmetic, never a copy.** Each shape's lamps come
//! from the per-shape resolvers in `map2d_resolve.rs` (the ones the device
//! runs today), so a part's segments carry the lamps today's
//! [`crate::resolve`] gives the same geometry, bit for bit. A path goes
//! through today's path (its moves as jumpers); a circle's ring k through a
//! single-ring ring set, except a ring of one lamp, which sits at the
//! centre; a repeat turns every lamp of copy k by today's
//! [`crate::Rotation2d`] in today's order, copy 0 included.

use alloc::collections::BTreeSet;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::map2d_doc::Map2dShape;
use crate::map2d_error::Map2dError;
use crate::map2d_resolve::{
    Rotation2d, filled_polygon_cells, resolve_filled_polygon, resolve_grid, resolve_path,
    resolve_polygon, resolve_ring, shape_lamp_count,
};
use crate::part::circle::Circle;
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

/// Render a mapping: its parts as one tree, its segments' lamps in one
/// buffer of exactly the size they need.
pub fn render(mapping: &Mapping) -> Result<Tree, RenderError> {
    let mut seen = BTreeSet::new();
    for part in mapping.all_parts() {
        if !seen.insert(part.id.as_str()) {
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

    let mut tree = Tree {
        nodes: Vec::new(),
        segments: Vec::new(),
        positions,
    };
    for part in &mapping.parts {
        render_part(&mut tree, part, None, &mapping.lamp)?;
    }
    debug_assert_eq!(
        tree.positions.len() as u64,
        total,
        "lamp_count drifted from the render: the exact-size reserve is broken"
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
        PartKind::Shape(shape) => render_shape(tree, &part.id, shape, me, &lamp)?,
        PartKind::Group { parts } => {
            for child in parts {
                render_part(tree, child, Some(me), &lamp)?;
            }
        }
        PartKind::Repeat(repeat) => render_repeat(tree, &part.id, repeat, me, &lamp)?,
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
) -> Result<(), RenderError> {
    let invalid = |reason: &str| Map2dError::InvalidObject {
        object: 0,
        name: id.to_string(),
        reason: reason.to_string(),
    };
    let fail = |e| RenderError::from_shape(id, e);
    let lamps = match shape {
        Shape::Path(path) => return render_path(tree, id, path, me, lamp),
        Shape::Circle(circle) => return render_circle(tree, id, circle, me, lamp),
        Shape::Polygon(polygon) => resolve_polygon(polygon, &invalid).map_err(fail)?,
        Shape::RingSet(ring) => resolve_ring(ring, &invalid).map_err(fail)?,
        Shape::Grid(grid) => resolve_grid(grid, &invalid).map_err(fail)?,
        Shape::FilledPolygon(filled) => resolve_filled_polygon(filled, &invalid).map_err(fail)?,
    };
    let segment = push_segment(tree, TreeId::of(id), &lamps, lamp);
    tree.nodes[me].segment = Some(segment);
    Ok(())
}

/// A path: today's path's lamps, as one segment at the path's own id, or
/// cut into one segment per subpath (`s1`, `s2`…) under it.
fn render_path(
    tree: &mut Tree,
    id: &PartId,
    path: &Path,
    me: usize,
    lamp: &LampRef,
) -> Result<(), RenderError> {
    let invalid = |reason: &str| Map2dError::InvalidObject {
        object: 0,
        name: id.to_string(),
        reason: reason.to_string(),
    };
    let today = path
        .to_path_shape()
        .map_err(|reason| RenderError::InvalidPart {
            part: id.clone(),
            reason: reason.to_string(),
        })?;
    let lamps = resolve_path(&today, &invalid).map_err(|e| RenderError::from_shape(id, e))?;
    match path.split {
        PathSplit::One => {
            let segment = push_segment(tree, TreeId::of(id), &lamps, lamp);
            tree.nodes[me].segment = Some(segment);
        }
        PathSplit::Subpaths => {
            for run in subpath_runs(&today) {
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

/// A circle: one segment per ring, in the circle's ring order. A ring of
/// one lamp is one lamp at the centre, whatever its radius.
fn render_circle(
    tree: &mut Tree,
    id: &PartId,
    circle: &Circle,
    me: usize,
    lamp: &LampRef,
) -> Result<(), RenderError> {
    let invalid = |reason: &str| Map2dError::InvalidObject {
        object: 0,
        name: id.to_string(),
        reason: reason.to_string(),
    };
    for ring in circle.rings_in_order() {
        let lamps = if ring.count == 1 {
            alloc::vec![circle.center]
        } else {
            resolve_ring(&circle.ring_shape(ring), &invalid)
                .map_err(|e| RenderError::from_shape(id, e))?
        };
        push_leaf(
            tree,
            TreeId::generated(id, ring.key.clone()),
            me,
            &lamps,
            lamp,
        );
    }
    Ok(())
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
    render_part(tree, &repeat.part, Some(me), lamp)?;
    let lamps = tree.positions.len() - first_lamp;
    let nodes = tree.nodes.len() - first_node;
    let segments = tree.segments.len() - first_segment;
    let degrees = repeat.copy_degrees();

    // Copies 1.. from copy 0 as rendered (not yet turned, not yet in a copy).
    for (k, &turn) in degrees.iter().enumerate().skip(1) {
        let rotation = Rotation2d::about(repeat.center, turn);
        for i in 0..lamps {
            let position = tree.positions[first_lamp + i];
            tree.positions.push(rotation.apply(position));
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
    let rotation = Rotation2d::about(repeat.center, degrees[0]);
    for position in &mut tree.positions[first_lamp..first_lamp + lamps] {
        *position = rotation.apply(*position);
    }
    for node in &mut tree.nodes[first_node..first_node + nodes] {
        node.id.copies.push(0);
    }
    for segment in &mut tree.segments[first_segment..first_segment + segments] {
        segment.id.copies.push(0);
    }
    Ok(())
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

fn shape_count(shape: &Shape) -> u64 {
    match shape {
        Shape::Path(path) => u64::from(path.count),
        Shape::Polygon(polygon) => u64::from(polygon.count),
        Shape::Circle(circle) => circle.rings.iter().map(|r| u64::from(r.count)).sum(),
        Shape::RingSet(ring) => u64::from(shape_lamp_count(&Map2dShape::Ring(ring.clone()))),
        Shape::Grid(grid) => u64::from(grid.cols) * u64::from(grid.rows),
        Shape::FilledPolygon(filled) => filled_polygon_cells(filled).len() as u64,
    }
}
