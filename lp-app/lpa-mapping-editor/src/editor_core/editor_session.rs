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
    RepeatShape, ResolvedMap2d, RingDir, RingOrder, RingShape, Rotation2d, bounds_of_points,
    resolve,
};

use crate::editor_core::map_selection::MapSelection;
use crate::editor_core::map_tool::MapTool;

const UNDO_CAP: usize = 100;

/// Default lamp pitch used by creation defaults and path-count derivation
/// (doc-space units; the spike's value).
pub const DEFAULT_PITCH: f32 = 26.0;

/// Instances a freshly authored repeat starts with — enough to read as a
/// wheel at a glance, and the dome's own sector count.
pub const DEFAULT_REPEAT_COUNT: u32 = 5;

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
    ///
    /// Through a repeat this edits the **inner** path — the handles sit on
    /// instance 0, and every other instance is that same path turned, so they
    /// follow live.
    pub fn move_vertex_from_gesture(&mut self, vertex: usize, position: [f32; 2]) {
        let Some(base) = self.gesture_doc() else {
            return;
        };
        let Some(index) = self.selection.single() else {
            return;
        };
        self.doc = base;
        if let Some(path) = self
            .doc
            .objects
            .get_mut(index)
            .and_then(|object| editable_path_mut(&mut object.shape))
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
            if let Some(path) = doc
                .objects
                .get_mut(index)
                .and_then(|object| editable_path_mut(&mut object.shape))
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
                if let Some(path) = doc
                    .objects
                    .get_mut(index)
                    .and_then(|object| editable_path_mut(&mut object.shape))
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
    ///
    /// A [`RepeatShape`] expands differently — into one independent object per
    /// instance, see [`Self::expand_repeat`] — because baking its whole wheel
    /// into a single path would fuse N physical strands into one run.
    pub fn expand_object(&mut self, index: usize) {
        if matches!(
            self.doc.objects.get(index).map(|object| &object.shape),
            Some(Map2dShape::Repeat(_))
        ) {
            self.expand_repeat(index);
            return;
        }
        let positions: Vec<[f32; 2]> = {
            let resolved = self.resolved();
            // The object's WHOLE lamp range, strands merged: `spans[index]` is
            // only the object's first strand once a repeat is in the document.
            let Some(span) = resolved.object_span(index as u32) else {
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

    /// Expand a repeat into one independent object per instance
    /// (`{name}-1`…`{name}-N`), in wiring order, selecting the new objects.
    ///
    /// This is the on-playa move: the wheel stops being parametric so a single
    /// strand can be nudged where the physical dome disagrees with the model,
    /// and *nothing else changes* — the lamps, their order, and their strand
    /// boundaries are the same before and after. Instances therefore stay
    /// parametric wherever the turn is representable (a path's points rotate,
    /// a ring's center rotates and its start angle shifts, a nested repeat's
    /// inner shape and center both rotate); only a grid, which has no rotation
    /// of its own, bakes down to a path through its resolved lamps.
    pub fn expand_repeat(&mut self, index: usize) {
        let Some(Map2dShape::Repeat(repeat)) = self
            .doc
            .objects
            .get(index)
            .map(|object| object.shape.clone())
        else {
            return;
        };
        let name = self.doc.objects[index].name.clone();
        let count = repeat.count.max(1);
        // Instance lamps come from the live resolution, so a shape that cannot
        // rotate itself bakes to exactly the lamps the repeat produced.
        let instance_positions: Vec<Vec<[f32; 2]>> = {
            let resolved = self.resolved();
            let Some(span) = resolved.object_span(index as u32) else {
                return;
            };
            let per_instance = span.count / count;
            (0..count)
                .map(|instance| {
                    let start = (span.start + instance * per_instance) as usize;
                    resolved.lamps[start..start + per_instance as usize]
                        .iter()
                        .map(|lamp| lamp.pos)
                        .collect()
                })
                .collect()
        };
        let objects: Vec<Map2dObject> = (0..count)
            .map(|instance| {
                let rotation = Rotation2d::about(repeat.center, repeat.instance_degrees(instance));
                let degrees = repeat.instance_degrees(instance);
                let shape = rotate_shape(&repeat.shape, rotation, degrees)
                    .unwrap_or_else(|| bake_path(&instance_positions[instance as usize]));
                Map2dObject {
                    name: format!("{name}-{}", instance + 1),
                    shape,
                }
            })
            .collect();
        self.edit(move |doc| {
            doc.objects.splice(index..=index, objects);
        });
        self.selection.objects = (index..index + count as usize).collect();
        self.selection.vertex = None;
    }

    /// Wrap one object's shape in a rotational repeat about the canvas center
    /// (falling back to the document's lamp bounds), `count` instances.
    ///
    /// The object keeps its slot in the wiring order and its selection: what
    /// was one strand is now `count` strands of the same shape, and the rail
    /// row still names one object.
    pub fn repeat_object(&mut self, index: usize, count: u32) {
        let Some(center) = self.default_repeat_center() else {
            return;
        };
        self.edit(move |doc| {
            if let Some(object) = doc.objects.get_mut(index) {
                let inner = object.shape.clone();
                object.shape = Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(inner),
                    center,
                    count,
                });
            }
        });
    }

    /// Unwrap one level of repeat: the object keeps instance 0's shape and
    /// loses the other instances. The inverse of [`Self::repeat_object`].
    pub fn unwrap_repeat(&mut self, index: usize) {
        self.edit(|doc| {
            if let Some(object) = doc.objects.get_mut(index)
                && let Map2dShape::Repeat(repeat) = &object.shape
            {
                object.shape = (*repeat.shape).clone();
            }
        });
    }

    /// Where a new repeat turns about: the authored canvas rect's center when
    /// the document has one (the frame the author composed in), else the
    /// center of everything resolved.
    fn default_repeat_center(&mut self) -> Option<[f32; 2]> {
        let bounds = self
            .doc
            .canvas_bounds()
            .or_else(|| bounds_of_points(&self.resolved().positions()))?;
        Some([
            bounds.min_x + bounds.width / 2.0,
            bounds.min_y + bounds.height / 2.0,
        ])
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

/// The path a shape offers for vertex editing: its own, or — through any
/// number of repeats — the inner path every instance is a turned copy of.
///
/// Editing instance 0 *is* editing the repeat: handles sit on the authored
/// geometry and the other instances follow on the next resolve.
fn editable_path_mut(shape: &mut Map2dShape) -> Option<&mut PathShape> {
    match shape {
        Map2dShape::Path(path) => Some(path),
        Map2dShape::Repeat(repeat) => editable_path_mut(&mut repeat.shape),
        _ => None,
    }
}

/// The same path, read-only — what the canvas draws handles and jumper lines
/// on.
#[must_use]
pub fn editable_path(shape: &Map2dShape) -> Option<&PathShape> {
    match shape {
        Map2dShape::Path(path) => Some(path),
        Map2dShape::Repeat(repeat) => editable_path(&repeat.shape),
        _ => None,
    }
}

/// One instance of a shape under a repeat's turn, still parametric.
///
/// `None` for a grid: it has no rotation of its own, so a turned grid is only
/// representable as baked geometry. Everything else carries the turn in its
/// own parameters — which is what keeps an expanded instance resolving to the
/// lamps the repeat produced instead of a re-sampled approximation of them.
fn rotate_shape(shape: &Map2dShape, rotation: Rotation2d, degrees: f32) -> Option<Map2dShape> {
    Some(match shape {
        Map2dShape::Path(path) => Map2dShape::Path(PathShape {
            points: path.points.iter().map(|p| rotation.apply(*p)).collect(),
            count: path.count,
            reversed: path.reversed,
            gaps: path.gaps.clone(),
        }),
        Map2dShape::Ring(ring) => Map2dShape::Ring(RingShape {
            center: rotation.apply(ring.center),
            radius: ring.radius,
            outer_count: ring.outer_count,
            rings: ring.rings,
            counts: ring.counts.clone(),
            order: ring.order,
            start_angle_deg: ring.start_angle_deg + degrees,
            dir: ring.dir,
        }),
        // Turning a wheel of wheels turns the hub and the spoke together: a
        // rotation conjugated by another rotation is the same rotation about
        // the moved center, so the inner instances land where they did.
        Map2dShape::Repeat(repeat) => Map2dShape::Repeat(RepeatShape {
            shape: Box::new(rotate_shape(&repeat.shape, rotation, degrees)?),
            center: rotation.apply(repeat.center),
            count: repeat.count,
        }),
        Map2dShape::Grid(_) => return None,
    })
}

/// A plain path through already-resolved lamp positions (the bake fallback).
fn bake_path(positions: &[[f32; 2]]) -> Map2dShape {
    Map2dShape::Path(PathShape {
        points: positions.to_vec(),
        count: (positions.len() as u32).max(1),
        reversed: false,
        gaps: Vec::new(),
    })
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
        // Moving a repeat moves the whole wheel: the inner shape and the
        // center it turns about travel together, so the instances land where
        // they looked like they would.
        Map2dShape::Repeat(repeat) => {
            translate_shape(&mut repeat.shape, dx, dy);
            repeat.center[0] += dx;
            repeat.center[1] += dy;
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
        // Same anchor for the inner shape and the center, so the wheel scales
        // rigidly — scaling the inner shape alone would slide every instance
        // off its rotation.
        Map2dShape::Repeat(repeat) => {
            scale_shape(&mut repeat.shape, anchor, factor);
            repeat.center = scale_point(repeat.center);
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
        sanitize_shape(&mut object.shape);
    }
}

/// Clamp one shape, recursing into [`RepeatShape`](lpc_mapping::RepeatShape) inners — a repeat's inner
/// shape is authored through the same fields as a top-level one and gets the
/// same guarantees.
fn sanitize_shape(shape: &mut Map2dShape) {
    match shape {
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
        // `count` is bounded at both ends: 0 instances resolve to nothing, and
        // an unbounded count multiplies the whole inner shape — a typo in a
        // number field should not turn a 300-lamp strand into a six-figure
        // document. `MAX_REPEAT_COUNT` is the shared ceiling.
        Map2dShape::Repeat(repeat) => {
            repeat.clamp_count();
            sanitize_shape(&mut repeat.shape);
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
    use lpc_mapping::{RepeatShape, corpus};

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

    // ---- rotational repeat ----------------------------------------------

    /// Sanitize's contract reaches inside a repeat: the instance count is
    /// bounded at both ends, and the inner shape gets the same clamps a
    /// top-level shape would.
    #[test]
    fn sanitize_clamps_repeat_count_and_recurses_into_the_inner_shape() {
        let mut session = session_with_repeat(5);
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Repeat(repeat) = shape {
                repeat.count = 0;
                if let Map2dShape::Path(path) = repeat.shape.as_mut() {
                    path.count = 0;
                    path.gaps = vec![0, 1, 2, 9];
                }
            }
        });
        let repeat = repeat_of(&session);
        assert_eq!(repeat.count, 1, "zero instances resolve to nothing");
        let Map2dShape::Path(path) = repeat.shape.as_ref() else {
            panic!("expected an inner path");
        };
        assert_eq!(path.count, 1);
        // Inner gaps are sorted, clamped to real segments, and never all of
        // them — the same guarantees a top-level path gets.
        assert_eq!(path.gaps, vec![0, 1]);
        assert!(session.resolve_error.is_none());

        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Repeat(repeat) = shape {
                repeat.count = 5_000;
            }
        });
        assert_eq!(repeat_of(&session).count, lpc_mapping::MAX_REPEAT_COUNT);
    }

    /// A repeat is a format-2 construct: committing one stamps 2, and
    /// unwrapping it drops the document back to 1.
    #[test]
    fn a_repeat_stamps_format_two_and_releases_it() {
        let mut session = session_with_repeat(5);
        assert_eq!(session.doc().format, 2);
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Repeat(repeat) = shape {
                *shape = (*repeat.shape).clone();
            }
        });
        assert_eq!(session.doc().format, 1);
    }

    /// Dragging a repeat moves the whole wheel — inner shape and rotation
    /// center together — so the lamps land exactly where the drag showed.
    #[test]
    fn moving_a_repeat_carries_its_center() {
        let mut session = session_with_repeat(4);
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        session.selection.select_only(0);
        session.begin_gesture();
        session.move_selected_from_gesture(30.0, -12.0);
        session.commit_gesture();

        assert_eq!(repeat_of(&session).center, [130.0, 88.0]);
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        assert_eq!(after.len(), before.len());
        for (index, (a, b)) in after.iter().zip(&before).enumerate() {
            assert!(
                (a[0] - (b[0] + 30.0)).abs() < 1e-3 && (a[1] - (b[1] - 12.0)).abs() < 1e-3,
                "lamp {index}: {a:?} is not {b:?} translated"
            );
        }
    }

    /// Resizing a repeat scales the wheel rigidly: lamp count is invariant and
    /// the bbox scales, which it would not if the center stayed put.
    #[test]
    fn scaling_a_repeat_scales_the_whole_wheel() {
        let mut session = session_with_repeat(4);
        session.select_all();
        let before = session.content_bounds().unwrap();
        session.begin_gesture();
        session.scale_selected_from_gesture([0.0, 0.0], 2.0);
        session.commit_gesture();
        let after = session.content_bounds().unwrap();
        assert!((after.width - before.width * 2.0).abs() < 0.5);
        assert!((after.height - before.height * 2.0).abs() < 0.5);
        assert_eq!(repeat_of(&session).center, [200.0, 200.0]);
        assert_eq!(session.lamp_count(), 16);
    }

    /// Wrapping a shape in a repeat is one undo step, turns about the canvas
    /// center, and multiplies the strands without touching the wiring order.
    #[test]
    fn repeat_around_a_point_wraps_one_object_in_one_step() {
        let mut session = session_with_gapped_path();
        session.edit(|doc| doc.canvas = Some([0.0, 0.0, 40.0, 40.0]));
        let before = session.lamp_count();
        session.selection.select_only(0);
        session.repeat_object(0, DEFAULT_REPEAT_COUNT);

        let repeat = repeat_of(&session);
        assert_eq!(repeat.count, DEFAULT_REPEAT_COUNT);
        assert_eq!(repeat.center, [20.0, 20.0], "the authored canvas center");
        assert!(matches!(repeat.shape.as_ref(), Map2dShape::Path(_)));
        assert_eq!(session.lamp_count(), before * DEFAULT_REPEAT_COUNT);
        assert_eq!(session.resolved().spans.len(), 5);
        assert_eq!(session.doc().objects.len(), 1, "still one object");
        assert_eq!(session.doc().format, 2);

        // One step out, and the document is byte-for-byte what it was.
        session.undo();
        assert!(matches!(
            session.doc().objects[0].shape,
            Map2dShape::Path(_)
        ));
        assert_eq!(session.lamp_count(), before);
    }

    /// Without an authored canvas the turn centers on what the document
    /// actually resolves to, so a freshly drawn path repeats about itself
    /// rather than about the origin.
    #[test]
    fn a_repeat_without_a_canvas_turns_about_the_content_center() {
        let mut session = session_with_gapped_path();
        session.repeat_object(0, 4);
        // The gapped-path fixture spans [0,0]..[20,10].
        assert_eq!(repeat_of(&session).center, [10.0, 5.0]);
    }

    /// Unwrap is the inverse of wrap: instance 0's shape survives, the other
    /// instances go, and the document drops back to format 1.
    #[test]
    fn unwrap_keeps_instance_zero_and_releases_the_format() {
        let mut session = session_with_repeat(5);
        let instance_zero: Vec<[f32; 2]> = session.resolved().lamps[..4]
            .iter()
            .map(|lamp| lamp.pos)
            .collect();
        session.unwrap_repeat(0);
        assert!(matches!(
            session.doc().objects[0].shape,
            Map2dShape::Path(_)
        ));
        assert_eq!(session.doc().format, 1);
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        assert_eq!(after, instance_zero);
        session.undo();
        assert_eq!(repeat_of(&session).count, 5);
    }

    /// **The on-playa contract.** Expanding a repeat replaces it with one
    /// object per instance, and the device output is untouched: same lamps, in
    /// the same order, in the same strands. Only then is nudging one strand a
    /// safe field repair.
    #[test]
    fn expanding_a_repeat_moves_no_lamp() {
        // The mini-dome: one gapped sector strand, five instances.
        let mut session = MapEditorSession::new(corpus::repeated_sector());
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        let spans_before: Vec<(u32, u32)> = session
            .resolved()
            .spans
            .iter()
            .map(|span| (span.start, span.count))
            .collect();

        session.expand_object(0);

        assert_eq!(session.doc().objects.len(), 5);
        let names: Vec<&str> = session
            .doc()
            .objects
            .iter()
            .map(|object| object.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["sector-1", "sector-2", "sector-3", "sector-4", "sector-5"]
        );
        // Selection lands on the new objects, one undo step behind.
        assert_eq!(session.selection.objects, (0..5).collect::<BTreeSet<_>>());
        assert!(session.can_undo());

        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        assert_eq!(after.len(), before.len());
        // Rotation is the resolver's own (`Rotation2d` + `instance_degrees`),
        // so the only difference is which side of the center subtraction each
        // arithmetic happens on: an expanded path's points are absolute, while
        // the repeat's instance 0 still went through `center + (p - center)`.
        // That is a last-bit difference — the printed worst case below is a
        // couple of ulps at doc-space magnitudes in the hundreds, ~1e-5 of a
        // lamp pitch — not a re-sampling of the geometry.
        let worst = after
            .iter()
            .zip(&before)
            .map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs()))
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-3, "lamps moved by up to {worst} doc units");
        let spans_after: Vec<(u32, u32)> = session
            .resolved()
            .spans
            .iter()
            .map(|span| (span.start, span.count))
            .collect();
        assert_eq!(spans_after, spans_before, "strand boundaries moved");
        // Each instance kept the jumper the author marked, and the expanded
        // geometry no longer needs the repeat construct — but gaps still do.
        assert_eq!(session.doc().format, 2);

        session.undo();
        assert_eq!(session.doc().objects.len(), 1);
        assert_eq!(repeat_of(&session).count, 5);
    }

    /// Expand stays parametric wherever the turn is representable: a repeated
    /// ring expands to rings (turned center + shifted start angle), not to
    /// baked polylines — so the objects stay editable as what they are.
    #[test]
    fn expanding_a_repeat_of_rings_keeps_them_rings() {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "pod".to_string(),
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Ring(RingShape {
                    center: [100.0, 40.0],
                    radius: 20.0,
                    outer_count: 8,
                    rings: 1,
                    counts: Vec::new(),
                    order: RingOrder::OuterFirst,
                    start_angle_deg: -90.0,
                    dir: RingDir::Cw,
                })),
                center: [100.0, 100.0],
                count: 6,
            }),
        });
        let mut session = MapEditorSession::new(doc);
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        session.expand_object(0);
        assert_eq!(session.doc().objects.len(), 6);
        for object in &session.doc().objects {
            assert!(matches!(object.shape, Map2dShape::Ring(_)), "still a ring");
        }
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        assert_eq!(after.len(), before.len());
        for (index, (a, b)) in after.iter().zip(&before).enumerate() {
            assert!(
                (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
                "lamp {index} moved: {a:?} vs {b:?}"
            );
        }
    }

    /// A grid has no rotation of its own, so a turned instance is only
    /// representable as baked geometry — the one case expand falls back to a
    /// path through the instance's own resolved lamps.
    #[test]
    fn expanding_a_repeat_of_grids_bakes_the_turned_instances() {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "tile".to_string(),
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Grid(GridShape {
                    origin: [100.0, 20.0],
                    cols: 4,
                    rows: 2,
                    pitch: 10.0,
                    routing: GridRouting::Snake,
                    start_corner: GridCorner::Tl,
                })),
                center: [100.0, 100.0],
                count: 4,
            }),
        });
        let mut session = MapEditorSession::new(doc);
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        session.expand_object(0);
        assert_eq!(session.doc().objects.len(), 4);
        for object in &session.doc().objects {
            assert!(matches!(object.shape, Map2dShape::Path(_)), "baked");
        }
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        assert_eq!(after.len(), before.len());
        for (index, (a, b)) in after.iter().zip(&before).enumerate() {
            assert!(
                (a[0] - b[0]).abs() < 1e-2 && (a[1] - b[1]).abs() < 1e-2,
                "lamp {index} moved: {a:?} vs {b:?}"
            );
        }
    }

    /// A wheel of wheels expands into wheels: the inner repeat's own center
    /// turns with it, so the strand structure survives instead of collapsing
    /// into one long baked run.
    #[test]
    fn expanding_a_nested_repeat_keeps_the_inner_strands() {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "wheel".to_string(),
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(Map2dShape::Path(PathShape {
                        points: vec![[100.0, 40.0], [100.0, 20.0]],
                        count: 3,
                        reversed: false,
                        gaps: Vec::new(),
                    })),
                    center: [100.0, 60.0],
                    count: 3,
                })),
                center: [100.0, 100.0],
                count: 2,
            }),
        });
        let mut session = MapEditorSession::new(doc);
        let before: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        let spans_before: Vec<(u32, u32)> = session
            .resolved()
            .spans
            .iter()
            .map(|span| (span.start, span.count))
            .collect();
        session.expand_object(0);
        assert_eq!(session.doc().objects.len(), 2);
        for object in &session.doc().objects {
            assert!(matches!(object.shape, Map2dShape::Repeat(_)));
        }
        let after: Vec<[f32; 2]> = session.resolved().lamps.iter().map(|l| l.pos).collect();
        for (index, (a, b)) in after.iter().zip(&before).enumerate() {
            assert!(
                (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3,
                "lamp {index} moved: {a:?} vs {b:?}"
            );
        }
        let spans_after: Vec<(u32, u32)> = session
            .resolved()
            .spans
            .iter()
            .map(|span| (span.start, span.count))
            .collect();
        assert_eq!(spans_after, spans_before, "6 strands before and after");
    }

    /// Vertex handles on a repeat edit instance 0's path; every other instance
    /// is that path turned, so they all follow the one drag.
    #[test]
    fn dragging_a_vertex_through_a_repeat_moves_every_instance() {
        let mut session = session_with_repeat(4);
        session.selection.select_only(0);
        session.begin_gesture();
        session.move_vertex_from_gesture(0, [110.0, 30.0]);
        session.commit_gesture();

        let Map2dShape::Path(path) = repeat_of(&session).shape.as_ref() else {
            panic!("expected an inner path");
        };
        assert_eq!(path.points[0], [110.0, 30.0]);
        // Instance 0's first lamp sits on the moved vertex, and instance 2 —
        // half a turn away — is its mirror about the center.
        assert_eq!(session.resolved().lamps[0].pos, [110.0, 30.0]);
        let opposite = session.resolved().lamps[8].pos;
        assert!(
            (opposite[0] - 90.0).abs() < 1e-3 && (opposite[1] - 170.0).abs() < 1e-3,
            "instance 2 lamp at {opposite:?}"
        );
        session.undo();
        assert_eq!(session.resolved().lamps[0].pos, [100.0, 40.0]);
    }

    /// Instance count is a field like any other: typing one commits one step
    /// and the strands follow.
    #[test]
    fn editing_the_instance_count_restrands_the_object() {
        let mut session = session_with_repeat(4);
        session.edit_object_shape(0, |shape| {
            if let Map2dShape::Repeat(repeat) = shape {
                repeat.count = 7;
            }
        });
        assert_eq!(session.resolved().spans.len(), 7);
        assert_eq!(session.lamp_count(), 28);
        session.undo();
        assert_eq!(session.resolved().spans.len(), 4);
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

    /// One four-lamp rib repeated `count` times about `[100, 100]`.
    fn session_with_repeat(count: u32) -> MapEditorSession {
        let mut doc = Map2dDoc::new();
        doc.objects.push(Map2dObject {
            name: "sector".to_string(),
            shape: Map2dShape::Repeat(RepeatShape {
                shape: Box::new(Map2dShape::Path(PathShape {
                    points: vec![[100.0, 40.0], [100.0, 10.0], [130.0, 10.0], [130.0, 40.0]],
                    count: 4,
                    reversed: false,
                    gaps: Vec::new(),
                })),
                center: [100.0, 100.0],
                count,
            }),
        });
        let mut session = MapEditorSession::new(doc);
        // Open the document through a commit so it carries the stamp the
        // editor would have written.
        session.rename_object(0, "sector".to_string());
        session.edit(|doc| doc.normalize_format());
        session
    }

    fn repeat_of(session: &MapEditorSession) -> &RepeatShape {
        let Map2dShape::Repeat(repeat) = &session.doc().objects[0].shape else {
            panic!("expected repeat");
        };
        repeat
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
