//! The editor's document session: doc + gesture-coalesced undo + selection +
//! tool state, all pure and host-tested.
//!
//! Undo is JSON-snapshot based (parametric documents are tiny — spike-proven):
//! a *gesture* (drag) captures one snapshot at pointer-down and commits one
//! undo step at pointer-up; discrete edits (`edit`) are one step each. During
//! a gesture, `*_from_gesture` ops re-derive the doc from the gesture
//! snapshot so incremental pointer events never accumulate drift.

use std::collections::BTreeSet;

use lpc_mapping::{
    Bounds2d, GridCorner, GridRouting, GridShape, Map2dDoc, Map2dObject, Map2dShape, PathShape,
    ResolvedMap2d, RingDir, RingOrder, RingShape, bounds_of_points, resolve,
};

use crate::editor_core::map_selection::MapSelection;
use crate::editor_core::map_tool::MapTool;

const UNDO_CAP: usize = 100;

/// Default lamp pitch used by creation defaults and path-count derivation
/// (doc-space units; the spike's value).
pub const DEFAULT_PITCH: f32 = 26.0;

pub struct MapEditorSession {
    doc: Map2dDoc,
    undo: Vec<String>,
    redo: Vec<String>,
    /// Snapshot captured at gesture start; present while a drag is live.
    gesture: Option<String>,
    /// Snapshot of the last externally persisted state (dirty tracking).
    saved: String,
    resolved: Option<ResolvedMap2d>,
    /// Human-readable resolve failure for the UI (kept, not panicked —
    /// session ops sanitize edits so this should stay `None`).
    pub resolve_error: Option<String>,
    pub selection: MapSelection,
    pub tool: MapTool,
}

impl MapEditorSession {
    #[must_use]
    pub fn new(doc: Map2dDoc) -> Self {
        let saved = doc.to_json();
        Self {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            saved,
            resolved: None,
            resolve_error: None,
            selection: MapSelection::default(),
            tool: MapTool::Select,
        }
    }

    // ---- document access -------------------------------------------------

    #[must_use]
    pub fn doc(&self) -> &Map2dDoc {
        &self.doc
    }

    /// Replace the whole document (open / scene load): fresh history,
    /// cleared selection, marks clean.
    pub fn set_doc(&mut self, doc: Map2dDoc) {
        self.doc = doc;
        self.undo.clear();
        self.redo.clear();
        self.gesture = None;
        self.saved = self.doc.to_json();
        self.selection.clear();
        self.tool = MapTool::Select;
        self.invalidate();
    }

    /// The resolved lamp list for the current document (memoized).
    pub fn resolved(&mut self) -> &ResolvedMap2d {
        if self.resolved.is_none() {
            match resolve(&self.doc) {
                Ok(resolved) => {
                    self.resolve_error = None;
                    self.resolved = Some(resolved);
                }
                Err(error) => {
                    self.resolve_error = Some(error.to_string());
                    self.resolved = Some(ResolvedMap2d {
                        lamps: Vec::new(),
                        spans: Vec::new(),
                    });
                }
            }
        }
        self.resolved.as_ref().expect("resolved cache filled above")
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.saved != self.doc.to_json()
    }

    /// Mark the current state as persisted (after a successful save).
    pub fn mark_saved(&mut self) {
        self.saved = self.doc.to_json();
    }

    /// Tight bounds of the resolved lamps, falling back to the canvas.
    pub fn content_bounds(&mut self) -> Option<Bounds2d> {
        let positions = self.resolved().positions();
        bounds_of_points(&positions).or_else(|| self.doc.canvas_bounds())
    }

    // ---- undo ------------------------------------------------------------

    /// Begin a drag gesture: captures the pre-gesture snapshot once.
    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.doc.to_json());
        }
    }

    /// End a drag gesture: pushes one undo step iff the doc changed.
    pub fn commit_gesture(&mut self) {
        if let Some(snapshot) = self.gesture.take()
            && snapshot != self.doc.to_json()
        {
            self.push_undo(snapshot);
        }
    }

    /// Gesture-scoped mutation WITHOUT an undo push: pairs with
    /// `begin_gesture`/`commit_gesture` so live typing in a property field
    /// previews immediately but lands as one undo step on commit.
    pub fn edit_uncommitted(&mut self, apply: impl FnOnce(&mut Map2dDoc)) {
        self.begin_gesture();
        apply(&mut self.doc);
        sanitize_doc(&mut self.doc);
        self.invalidate();
    }

    /// One-step discrete edit (property change, create, delete, reorder).
    pub fn edit(&mut self, apply: impl FnOnce(&mut Map2dDoc)) {
        let before = self.doc.to_json();
        apply(&mut self.doc);
        sanitize_doc(&mut self.doc);
        if before != self.doc.to_json() {
            self.push_undo(before);
            self.invalidate();
        }
    }

    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo.pop() {
            self.redo.push(self.doc.to_json());
            self.restore(&snapshot);
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(self.doc.to_json());
            self.restore(&snapshot);
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- selection -------------------------------------------------------

    pub fn select_all(&mut self) {
        self.selection.objects = (0..self.doc.objects.len()).collect();
        self.selection.vertex = None;
    }

    /// Select objects whose resolved lamps intersect a doc-space rect
    /// (`additive` keeps the existing selection).
    pub fn marquee_select(&mut self, min: [f32; 2], max: [f32; 2], additive: bool) {
        let mut hit = BTreeSet::new();
        for lamp in &self.resolved().lamps.clone() {
            if lamp.pos[0] >= min[0]
                && lamp.pos[0] <= max[0]
                && lamp.pos[1] >= min[1]
                && lamp.pos[1] <= max[1]
            {
                hit.insert(lamp.object as usize);
            }
        }
        if additive {
            self.selection.objects.extend(hit);
        } else {
            self.selection.objects = hit;
        }
        self.selection.vertex = None;
    }

    // ---- gesture-driven transforms --------------------------------------

    /// Translate the selection by a total delta from the gesture start.
    pub fn move_selected_from_gesture(&mut self, dx: f32, dy: f32) {
        let Some(base) = self.gesture_doc() else {
            return;
        };
        let selected = self.selection.objects.clone();
        self.doc = base;
        for index in selected {
            if let Some(object) = self.doc.objects.get_mut(index) {
                translate_shape(&mut object.shape, dx, dy);
            }
        }
        self.invalidate();
    }

    /// Uniformly scale the selection about `anchor` by a total factor from
    /// the gesture start.
    pub fn scale_selected_from_gesture(&mut self, anchor: [f32; 2], factor: f32) {
        let Some(base) = self.gesture_doc() else {
            return;
        };
        let factor = factor.clamp(0.05, 50.0);
        let selected = self.selection.objects.clone();
        self.doc = base;
        for index in selected {
            if let Some(object) = self.doc.objects.get_mut(index) {
                scale_shape(&mut object.shape, anchor, factor);
            }
        }
        sanitize_doc(&mut self.doc);
        self.invalidate();
    }

    /// Move one vertex of the single selected path (gesture-driven).
    pub fn move_vertex_from_gesture(&mut self, vertex: usize, position: [f32; 2]) {
        let Some(base) = self.gesture_doc() else {
            return;
        };
        let Some(index) = self.selection.single() else {
            return;
        };
        self.doc = base;
        if let Some(Map2dShape::Path(path)) = self.doc.objects.get_mut(index).map(|o| &mut o.shape)
            && let Some(point) = path.points.get_mut(vertex)
        {
            *point = position;
        }
        self.invalidate();
    }

    // ---- structural edits ------------------------------------------------

    /// Insert a vertex into the selected path so it becomes `points[at]`,
    /// splitting the segment it lands in. Inert segments survive the split:
    /// both halves of a split jumper stay inert, and gaps after the insertion
    /// point shift up with their segments.
    pub fn insert_path_vertex(&mut self, index: usize, at: usize, position: [f32; 2]) {
        self.edit(|doc| {
            if let Some(Map2dShape::Path(path)) = doc.objects.get_mut(index).map(|o| &mut o.shape)
                && at <= path.points.len()
            {
                path.gaps = gaps_after_vertex_insert(&path.gaps, at);
                path.points.insert(at, position);
            }
        });
    }

    /// Delete the selected vertex (path keeps ≥ 2 points) or, without a
    /// vertex selection, all selected objects.
    pub fn delete_selection(&mut self) {
        if let (Some(index), Some(vertex)) = (self.selection.single(), self.selection.vertex) {
            let mut deleted = false;
            self.edit(|doc| {
                if let Some(Map2dShape::Path(path)) =
                    doc.objects.get_mut(index).map(|o| &mut o.shape)
                    && path.points.len() > 2
                    && vertex < path.points.len()
                {
                    path.gaps = gaps_after_vertex_delete(&path.gaps, vertex, path.points.len() - 1);
                    path.points.remove(vertex);
                    path.count = path.count.saturating_sub(1).max(2);
                    deleted = true;
                }
            });
            if deleted {
                self.selection.vertex = None;
                return;
            }
        }
        let selected = self.selection.objects.clone();
        if selected.is_empty() {
            return;
        }
        self.edit(|doc| {
            for index in selected.iter().rev() {
                if *index < doc.objects.len() {
                    doc.objects.remove(*index);
                }
            }
        });
        self.selection.clear();
    }

    /// Move an object one slot earlier/later in the wiring order.
    pub fn reorder_object(&mut self, from: usize, to: usize) {
        let len = self.doc.objects.len();
        if from >= len || to >= len || from == to {
            return;
        }
        self.edit(|doc| {
            let object = doc.objects.remove(from);
            doc.objects.insert(to, object);
        });
        // Selection follows the moved object; other indices shift around it.
        let remap = |index: usize| -> usize {
            if index == from {
                to
            } else if from < to && index > from && index <= to {
                index - 1
            } else if to < from && index >= to && index < from {
                index + 1
            } else {
                index
            }
        };
        self.selection.objects = self.selection.objects.iter().map(|i| remap(*i)).collect();
    }

    pub fn rename_object(&mut self, index: usize, name: String) {
        self.edit(|doc| {
            if let Some(object) = doc.objects.get_mut(index) {
                object.name = name;
            }
        });
    }

    /// One-undo-step shape edit for property fields; values are sanitized.
    pub fn edit_object_shape(&mut self, index: usize, apply: impl FnOnce(&mut Map2dShape)) {
        self.edit(|doc| {
            if let Some(object) = doc.objects.get_mut(index) {
                apply(&mut object.shape);
            }
        });
    }

    /// Illustrator-style expand: replace a parametric object with a plain
    /// path through its own resolved lamps, ready for hand-tweaking. The
    /// lamp layout is identical before and after.
    pub fn expand_object(&mut self, index: usize) {
        let positions: Vec<[f32; 2]> = {
            let resolved = self.resolved();
            let Some(span) = resolved.spans.get(index).copied() else {
                return;
            };
            resolved.lamps[span.start as usize..(span.start + span.count) as usize]
                .iter()
                .map(|lamp| lamp.pos)
                .collect()
        };
        if positions.len() < 2 {
            return;
        }
        let count = positions.len() as u32;
        self.edit(move |doc| {
            if let Some(object) = doc.objects.get_mut(index) {
                object.shape = Map2dShape::Path(PathShape {
                    points: positions,
                    count,
                    reversed: false,
                    gaps: Vec::new(),
                });
            }
        });
    }

    // ---- creation (parent decision D6: click drops defaults) -------------

    /// Drop a default 8×8 grid centered on `at`; selects it, tool → select.
    pub fn create_default_grid(&mut self, at: [f32; 2]) -> usize {
        let origin = [at[0] - 3.5 * DEFAULT_PITCH, at[1] - 3.5 * DEFAULT_PITCH];
        self.create_object(Map2dObject {
            name: self.next_name("grid"),
            shape: Map2dShape::Grid(GridShape {
                origin,
                cols: 8,
                rows: 8,
                pitch: DEFAULT_PITCH,
                routing: GridRouting::Snake,
                start_corner: GridCorner::Tl,
            }),
        })
    }

    /// Drop a default ring centered on `at`; selects it, tool → select.
    pub fn create_default_ring(&mut self, at: [f32; 2]) -> usize {
        self.create_object(Map2dObject {
            name: self.next_name("ring"),
            shape: Map2dShape::Ring(RingShape {
                center: at,
                radius: 80.0,
                outer_count: 19,
                rings: 1,
                counts: Vec::new(),
                order: RingOrder::OuterFirst,
                start_angle_deg: -90.0,
                dir: RingDir::Cw,
            }),
        })
    }

    // ---- path drafting ---------------------------------------------------

    pub fn path_add_point(&mut self, point: [f32; 2]) {
        if let MapTool::Path { draft } = &mut self.tool {
            draft.push(point);
        }
    }

    /// Escape during path drawing: removes ONE vertex (never wholesale —
    /// D6). Returns `false` when the draft was already empty.
    pub fn path_backout(&mut self) -> bool {
        if let MapTool::Path { draft } = &mut self.tool {
            draft.pop().is_some()
        } else {
            false
        }
    }

    #[must_use]
    pub fn path_draft(&self) -> &[[f32; 2]] {
        match &self.tool {
            MapTool::Path { draft } => draft,
            _ => &[],
        }
    }

    /// Finish the draft: creates a path object (count from arc length) when
    /// it has ≥ 2 distinct points, else just clears. Tool → select.
    pub fn path_finish(&mut self) -> Option<usize> {
        let MapTool::Path { draft } = std::mem::replace(&mut self.tool, MapTool::Select) else {
            return None;
        };
        let mut points: Vec<[f32; 2]> = Vec::new();
        for point in draft {
            let near_previous = points
                .last()
                .is_some_and(|last| distance(*last, point) <= 4.0);
            if !near_previous {
                points.push(point);
            }
        }
        if points.len() < 2 {
            return None;
        }
        let length: f32 = points
            .windows(2)
            .map(|pair| distance(pair[0], pair[1]))
            .sum();
        let count = ((length / DEFAULT_PITCH).round() as u32).max(2);
        Some(self.create_object(Map2dObject {
            name: self.next_name("path"),
            shape: Map2dShape::Path(PathShape {
                points,
                count,
                reversed: false,
                gaps: Vec::new(),
            }),
        }))
    }

    // ---- derived info ----------------------------------------------------

    pub fn lamp_count(&mut self) -> u32 {
        self.resolved().lamps.len() as u32
    }

    pub fn universe_count(&mut self) -> u32 {
        self.resolved().universe_count()
    }

    // ---- internals -------------------------------------------------------

    fn create_object(&mut self, object: Map2dObject) -> usize {
        self.edit(|doc| doc.objects.push(object));
        let index = self.doc.objects.len() - 1;
        self.selection.select_only(index);
        self.tool = MapTool::Select;
        index
    }

    fn next_name(&self, kind: &str) -> String {
        format!("{kind} {}", self.doc.objects.len() + 1)
    }

    fn gesture_doc(&self) -> Option<Map2dDoc> {
        let snapshot = self.gesture.as_ref()?;
        Map2dDoc::from_json(snapshot).ok()
    }

    fn push_undo(&mut self, snapshot: String) {
        self.undo.push(snapshot);
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, snapshot: &str) {
        if let Ok(doc) = Map2dDoc::from_json(snapshot) {
            self.doc = doc;
        }
        self.gesture = None;
        let len = self.doc.objects.len();
        self.selection.objects.retain(|index| *index < len);
        self.selection.vertex = None;
        self.invalidate();
    }

    fn invalidate(&mut self) {
        self.resolved = None;
    }
}

fn translate_shape(shape: &mut Map2dShape, dx: f32, dy: f32) {
    match shape {
        Map2dShape::Grid(grid) => {
            grid.origin[0] += dx;
            grid.origin[1] += dy;
        }
        Map2dShape::Ring(ring) => {
            ring.center[0] += dx;
            ring.center[1] += dy;
        }
        Map2dShape::Path(path) => {
            for point in &mut path.points {
                point[0] += dx;
                point[1] += dy;
            }
        }
    }
}

fn scale_shape(shape: &mut Map2dShape, anchor: [f32; 2], factor: f32) {
    let scale_point = |point: [f32; 2]| -> [f32; 2] {
        [
            anchor[0] + (point[0] - anchor[0]) * factor,
            anchor[1] + (point[1] - anchor[1]) * factor,
        ]
    };
    match shape {
        Map2dShape::Grid(grid) => {
            grid.origin = scale_point(grid.origin);
            grid.pitch *= factor;
        }
        Map2dShape::Ring(ring) => {
            ring.center = scale_point(ring.center);
            ring.radius *= factor;
        }
        Map2dShape::Path(path) => {
            for point in &mut path.points {
                *point = scale_point(*point);
            }
        }
    }
}

/// Clamp authored values so every session-produced document resolves, and
/// stamp the minimal format its content needs.
///
/// The stamp is part of every commit, not a save-time afterthought: whatever
/// the document declared when it was opened, what the editor emits declares
/// the *lowest* format able to read it. Strip the last newer construct out of
/// a document and it drops back to the older format, readable by older
/// firmware again.
fn sanitize_doc(doc: &mut Map2dDoc) {
    doc.normalize_format();
    for object in &mut doc.objects {
        match &mut object.shape {
            Map2dShape::Grid(grid) => {
                grid.cols = grid.cols.max(1);
                grid.rows = grid.rows.max(1);
                grid.pitch = grid.pitch.max(0.5);
            }
            Map2dShape::Ring(ring) => {
                ring.outer_count = ring.outer_count.max(1);
                ring.radius = ring.radius.max(1.0);
                ring.rings = ring.rings.max(1);
            }
            Map2dShape::Path(path) => {
                path.count = path.count.max(1);
                sanitize_path_gaps(path);
            }
        }
    }
}

/// Sort, dedupe and clamp inert segment indices.
///
/// Sanitize's contract is "every emitted document resolves", so a gap set that
/// would leave the path with nothing to light is trimmed rather than rejected:
/// indices naming no segment are dropped, and if every segment is marked
/// inert the last one gives way. Editing the field is thus never a dead end.
fn sanitize_path_gaps(path: &mut PathShape) {
    let segments = path.points.len().saturating_sub(1);
    path.gaps.retain(|gap| (*gap as usize) < segments);
    path.gaps.sort_unstable();
    path.gaps.dedup();
    if path.gaps.len() >= segments {
        path.gaps.pop();
    }
}

/// Inert indices after a vertex is inserted so it becomes `points[at]`.
///
/// The insertion splits segment `at - 1` in two (`at - 1` and `at`); a split
/// jumper is still a jumper, so both halves stay inert. Segments after the
/// split shift up by one.
fn gaps_after_vertex_insert(gaps: &[u32], at: usize) -> Vec<u32> {
    let split = at.checked_sub(1).map(|split| split as u32);
    let mut next = Vec::with_capacity(gaps.len() + 1);
    for gap in gaps {
        match split {
            Some(split) if *gap == split => {
                next.push(split);
                next.push(split + 1);
            }
            Some(split) if *gap < split => next.push(*gap),
            _ => next.push(gap + 1),
        }
    }
    next
}

/// Inert indices after `vertex` is removed from a path with `segments`
/// segments.
///
/// Deleting an interior vertex merges segments `vertex - 1` and `vertex` into
/// one; the merged segment stays inert if *either* half was, because the
/// alternative is silently lighting wire the author marked as jumper. Deleting
/// an endpoint drops its segment outright.
fn gaps_after_vertex_delete(gaps: &[u32], vertex: usize, segments: usize) -> Vec<u32> {
    let last_vertex = segments; // points.len() - 1
    let mut next = Vec::with_capacity(gaps.len());
    for gap in gaps {
        let gap = *gap as usize;
        let mapped = if vertex == 0 {
            (gap > 0).then(|| gap - 1)
        } else if vertex >= last_vertex {
            (gap + 1 < segments).then_some(gap)
        } else if gap + 1 < vertex {
            Some(gap)
        } else if gap == vertex - 1 || gap == vertex {
            Some(vertex - 1)
        } else {
            Some(gap - 1)
        };
        if let Some(mapped) = mapped {
            next.push(mapped as u32);
        }
    }
    next.sort_unstable();
    next.dedup();
    next
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::corpus;

    #[test]
    fn gesture_move_is_one_undo_step_without_drift() {
        let mut session = session_with_ring();
        session.selection.select_only(0);
        session.begin_gesture();
        // Many incremental pointer events; totals, not deltas.
        for step in 1..=10 {
            session.move_selected_from_gesture(step as f32 * 2.0, 0.0);
        }
        session.commit_gesture();
        assert_eq!(ring_center(&mut session), [260.0, 200.0]);
        assert!(session.can_undo());
        session.undo();
        assert_eq!(ring_center(&mut session), [240.0, 200.0]);
        session.redo();
        assert_eq!(ring_center(&mut session), [260.0, 200.0]);
    }

    #[test]
    fn unchanged_gesture_pushes_no_undo() {
        let mut session = session_with_ring();
        session.begin_gesture();
        session.commit_gesture();
        assert!(!session.can_undo());
    }

    #[test]
    fn scale_gesture_scales_all_selected_shapes() {
        let mut session = MapEditorSession::new(corpus::cat_ears());
        session.select_all();
        let before = session.content_bounds().unwrap();
        session.begin_gesture();
        session.scale_selected_from_gesture([before.min_x, before.min_y], 2.0);
        session.commit_gesture();
        let after = session.content_bounds().unwrap();
        assert!((after.width - before.width * 2.0).abs() < 0.5);
        assert!((after.height - before.height * 2.0).abs() < 0.5);
        // Lamp count is invariant under scaling.
        assert_eq!(session.lamp_count(), 48);
    }

    #[test]
    fn delete_remaps_selection_and_undo_restores() {
        let mut session = MapEditorSession::new(corpus::cat_ears());
        assert_eq!(session.doc().objects.len(), 3);
        session.selection.select_only(1);
        session.delete_selection();
        assert_eq!(session.doc().objects.len(), 2);
        assert!(session.selection.is_empty());
        session.undo();
        assert_eq!(session.doc().objects.len(), 3);
    }

    #[test]
    fn reorder_moves_wiring_order_and_selection_follows() {
        let mut session = MapEditorSession::new(corpus::cat_ears());
        let first_name = session.doc().objects[0].name.clone();
        session.selection.select_only(0);
        session.reorder_object(0, 2);
        assert_eq!(session.doc().objects[2].name, first_name);
        assert_eq!(session.selection.single(), Some(2));
    }

    #[test]
    fn marquee_selects_objects_with_lamps_in_rect() {
        let mut session = MapEditorSession::new(corpus::cat_ears());
        // The whole doc.
        session.marquee_select([0.0, 0.0], [1000.0, 1000.0], false);
        assert_eq!(session.selection.objects.len(), 3);
        // Nothing.
        session.marquee_select([-100.0, -100.0], [-50.0, -50.0], false);
        assert!(session.selection.is_empty());
    }

    #[test]
    fn create_defaults_select_and_return_to_select_tool() {
        let mut session = MapEditorSession::new(Map2dDoc::new());
        session.tool = MapTool::Grid;
        let index = session.create_default_grid([100.0, 100.0]);
        assert_eq!(index, 0);
        assert_eq!(session.selection.single(), Some(0));
        assert!(session.tool.is_select());
        assert_eq!(session.lamp_count(), 64);
        session.tool = MapTool::Ring;
        session.create_default_ring([300.0, 300.0]);
        assert_eq!(session.lamp_count(), 64 + 19);
        assert!(session.can_undo());
    }

    #[test]
    fn path_draft_backs_out_one_vertex_at_a_time() {
        let mut session = MapEditorSession::new(Map2dDoc::new());
        session.tool = MapTool::path();
        session.path_add_point([0.0, 0.0]);
        session.path_add_point([100.0, 0.0]);
        session.path_add_point([100.0, 100.0]);
        assert!(session.path_backout());
        assert_eq!(session.path_draft().len(), 2);
        let index = session.path_finish().expect("path created");
        let Map2dShape::Path(path) = &session.doc().objects[index].shape else {
            panic!("expected path");
        };
        assert_eq!(path.points.len(), 2);
        assert_eq!(path.count, 4); // 100 units / 26 pitch ≈ 4
    }

    #[test]
    fn short_path_draft_finishes_without_creating() {
        let mut session = MapEditorSession::new(Map2dDoc::new());
        session.tool = MapTool::path();
        session.path_add_point([0.0, 0.0]);
        session.path_add_point([1.0, 0.0]); // near-duplicate, dropped
        assert!(session.path_finish().is_none());
        assert!(session.doc().objects.is_empty());
        assert!(session.tool.is_select());
    }

    #[test]
    fn property_edit_sanitizes_and_is_undoable() {
        let mut session = session_with_ring();
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Ring(ring) = shape {
                ring.rings = 3;
                ring.counts = vec![16, 0, 4]; // 0 falls back to derived
            }
        });
        assert!(session.resolve_error.is_none());
        // Auto radii 90/60/30: 16 + derived(60→11) + forced 4.
        assert_eq!(session.lamp_count(), 16 + 11 + 4);
        session.undo();
        let Map2dShape::Ring(ring) = &session.doc().objects[0].shape else {
            panic!("expected ring");
        };
        assert_eq!(ring.rings, 2);
    }

    #[test]
    fn expand_turns_a_ring_into_an_identical_path() {
        let mut session = session_with_ring();
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        session.expand_object(0);
        let Map2dShape::Path(path) = &session.doc().objects[0].shape else {
            panic!("expected path after expand");
        };
        assert_eq!(path.points.len(), 24);
        assert_eq!(path.count, 24);
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        // Endpoints identical; interior lamps ride the polyline through the
        // original lamp positions.
        assert_eq!(before.first(), after.first());
        assert_eq!(before.last(), after.last());
        assert!(session.can_undo());
        session.undo();
        assert!(matches!(
            session.doc().objects[0].shape,
            Map2dShape::Ring(_)
        ));
    }

    /// Minimal stamping: what the editor commits declares the lowest format
    /// able to read it, whatever the document claimed on the way in.
    #[test]
    fn a_commit_stamps_the_minimal_required_format() {
        let mut doc = corpus::basic_button();
        doc.format = 9;
        let mut session = MapEditorSession::new(doc);
        session.rename_object(0, "renamed".to_string());
        assert_eq!(session.doc().format, 1);
        // The emitted body is therefore readable by this build again.
        assert!(Map2dDoc::from_json(&session.doc().to_json()).is_ok());
    }

    /// Sanitize's contract on gaps: sorted, deduped, no index naming a
    /// segment that does not exist, and never every segment at once.
    #[test]
    fn sanitize_orders_and_clamps_gap_indices() {
        let mut session = session_with_gapped_path();
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps = vec![2, 0, 2, 9];
            }
        });
        assert_eq!(gaps_of(&session), vec![0, 2]);
        assert!(session.resolve_error.is_none());

        // Every segment inert would leave nothing to light: the last gap gives
        // way rather than the edit failing.
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps = vec![0, 1, 2];
            }
        });
        assert_eq!(gaps_of(&session), vec![0, 1]);
        assert!(session.resolve_error.is_none());
        assert_eq!(session.lamp_count(), 4);
    }

    /// A gapped document stamps format 2 on commit, and drops back to 1 the
    /// moment the last gap goes.
    #[test]
    fn gaps_stamp_format_two_and_release_it() {
        let mut session = session_with_gapped_path();
        assert_eq!(session.doc().format, 1);
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps = vec![1];
            }
        });
        assert_eq!(session.doc().format, 2);
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps.clear();
            }
        });
        assert_eq!(session.doc().format, 1);
    }

    /// Inserting a vertex inside a jumper leaves both halves inert; segments
    /// after the insertion point shift up with their geometry.
    #[test]
    fn inserting_a_vertex_remaps_gaps_around_the_split() {
        let mut session = session_with_gapped_path();
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps = vec![1];
            }
        });
        // Split segment 1 (the jumper) at its midpoint: it becomes 1 and 2.
        session.insert_path_vertex(0, 2, [10.0, 5.0]);
        assert_eq!(gaps_of(&session), vec![1, 2]);
        assert_eq!(points_of(&session).len(), 5);

        // A vertex inserted before every gap pushes them all up one.
        session.insert_path_vertex(0, 0, [-10.0, 0.0]);
        assert_eq!(gaps_of(&session), vec![2, 3]);
    }

    /// Deleting a vertex merges the two segments it joined; a merged segment
    /// stays inert if either half was, so wire never silently lights up.
    #[test]
    fn deleting_a_vertex_remaps_gaps_and_keeps_wire_inert() {
        let mut session = session_with_gapped_path();
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Path(path) = shape {
                path.gaps = vec![1];
            }
        });
        // Vertex 2 joins segment 1 (the jumper) and segment 2; the merged
        // segment 1 is still a jumper.
        session.selection.select_only(0);
        session.selection.vertex = Some(2);
        session.delete_selection();
        assert_eq!(points_of(&session).len(), 3);
        assert_eq!(gaps_of(&session), vec![1]);

        // Deleting the first vertex drops segment 0 and shifts the rest down.
        session.selection.select_only(0);
        session.selection.vertex = Some(0);
        session.delete_selection();
        assert_eq!(points_of(&session).len(), 2);
        assert_eq!(gaps_of(&session), Vec::<u32>::new()); // clamped: 1 segment left, and it must light
    }

    #[test]
    fn dirty_tracks_saved_snapshot() {
        let mut session = session_with_ring();
        assert!(!session.is_dirty());
        session.edit(|doc| doc.objects[0].name = "renamed".to_string());
        assert!(session.is_dirty());
        session.mark_saved();
        assert!(!session.is_dirty());
    }

    fn session_with_ring() -> MapEditorSession {
        MapEditorSession::new(corpus::basic_button())
    }

    /// One path, four points, three 10-unit segments — the gap tests all read
    /// off this geometry.
    fn session_with_gapped_path() -> MapEditorSession {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "channel".to_string(),
            shape: Map2dShape::Path(PathShape {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [20.0, 10.0]],
                count: 4,
                reversed: false,
                gaps: Vec::new(),
            }),
        });
        MapEditorSession::new(doc)
    }

    fn path_of(session: &MapEditorSession) -> &PathShape {
        let Map2dShape::Path(path) = &session.doc().objects[0].shape else {
            panic!("expected path");
        };
        path
    }

    fn gaps_of(session: &MapEditorSession) -> Vec<u32> {
        path_of(session).gaps.clone()
    }

    fn points_of(session: &MapEditorSession) -> Vec<[f32; 2]> {
        path_of(session).points.clone()
    }

    fn ring_center(session: &mut MapEditorSession) -> [f32; 2] {
        let Map2dShape::Ring(ring) = &session.doc().objects[0].shape else {
            panic!("expected ring");
        };
        ring.center
    }
}
