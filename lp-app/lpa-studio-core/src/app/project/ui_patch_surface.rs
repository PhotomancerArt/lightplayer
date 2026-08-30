//! The project-scoped PATCH SURFACE (D36, slice 2): every output's ports
//! and cells, every producing fixture's runs and instances, one shared
//! selection — the full-page editing home the face bays are read-only
//! mirrors of.
//!
//! Built by the project controller from what the bay derivation already
//! computes, widened to project scope: the sidebar walks
//! [`UiPatchSurface::outputs`] as an output → port tree, the canvas draws
//! each output's lamps from its published frame (layout + bytes, the same
//! [`crate::UiControlProductPreview`] the bay cells read), and the bay
//! section reuses the two-sided cells verbatim. Instance structure (the
//! `/sector/2` grain) parses out of each fixture's OWN map2d document —
//! the same bytes its mapping editor holds — so the surface's grain is the
//! patch format's grain by construction.
//!
//! Read-only in P5: the DTO carries selection TARGETS (stable strings the
//! ui-state stores), not slot addresses. P6 fills in the verb actions.
//!
//! Since the unified-editor pass this surface is also the **editor shell's**
//! view DTO (extend, don't fork): the same fixtures/outputs feed the
//! workbench panels and the Arrange canvas, and the surface carries the
//! project-level `editor.json` facts — per-fixture [`UiArrangeMeta`] plus
//! the loaded-flag/artifact pair pages drive the editor-meta prefetch from
//! (never hand-code a fetch a flag doesn't ask for — the #409 lesson).

use lpc_model::NodeId;

use crate::{UiFixturePatch, UiPatchBay};

/// The whole surface, in module order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchSurface {
    /// Every module on the tree path above a fixture or output, in tree
    /// (pre-)order — the structural context the G1 feedback asked the
    /// Fixtures/Outputs trees to show. Fixtures and outputs point at their
    /// nearest enclosing module via [`UiPatchSurfaceFixture::module`] /
    /// [`UiPatchSurfaceOutput::module`].
    pub modules: Vec<UiPatchSurfaceModule>,
    /// Every bus-consuming output that has answered a frame probe, with its
    /// bay (ports + cells) — the sidebar's tree and the canvas's wires.
    pub outputs: Vec<UiPatchSurfaceOutput>,
    /// Every producing fixture on any of those wires, with its runs and its
    /// instance table.
    pub fixtures: Vec<UiPatchSurfaceFixture>,
    /// THE chase for the selected UNMAPPED object, computed once here (Q9)
    /// — the panel strip and the canvas sprites both paint these colors, so
    /// the two views can never disagree about what the object is doing.
    /// `None` whenever the selection is mapped (its chase is in the
    /// published bytes), wire-side, or absent. See
    /// [`crate::UiPatchChasePreview`].
    pub chase_preview: Option<crate::UiPatchChasePreview>,
    /// The project-level `editor.json` settled locally (cached content, or
    /// the file is known absent). While false, pages dispatch
    /// [`crate::EditorMetaFetchOp`] against [`Self::editor_meta_artifact`]
    /// and fixtures carry `arrange: None`.
    pub editor_meta_loaded: bool,
    /// A present-but-unreadable `editor.json` (newer format, parse error).
    /// Arrange edits refuse rather than rewrite over it.
    pub editor_meta_error: Option<String>,
    /// Where `editor.json` lives — the prefetch and write target.
    pub editor_meta_artifact: Option<lpc_model::ArtifactLocation>,
}

impl UiPatchSurface {
    /// Is nothing PATCHED here — no output carrying a cell?
    ///
    /// Never a reason to withhold the surface: an all-free surface is the
    /// walk-up flow's own starting point (every fixture manual and
    /// unmapped), and hiding it leaves no port space to click. The pages'
    /// empty state keys on the surface being absent — which means the
    /// project has no output at all — not on this.
    #[must_use]
    pub fn nothing_patched(&self) -> bool {
        self.outputs.iter().all(|output| output.bay.is_empty())
    }
}

/// One module level above the surface's fixtures/outputs: pure structural
/// context (label + nesting), selectable like every other surface row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchSurfaceModule {
    /// The module node — selection identity.
    pub node: NodeId,
    /// The node's tree label (the root module wears the project name).
    pub label: String,
    /// The node's address path — the prefix its fixtures/outputs nest under.
    pub address: Option<String>,
    /// Nesting depth among the surface's modules (root module = 0), for
    /// indentation without re-deriving prefixes in the view.
    pub depth: usize,
}

/// One output on the surface: identity, its bay, and its live wire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchSurfaceOutput {
    /// The output node — selection identity and the join key for cells.
    pub node: NodeId,
    /// The node's tree label (`out_a`).
    pub label: String,
    /// The authored `name` ("1", "Box 2"), when set — what patch entries
    /// reference and what every surface row SHOWS (falls back to the
    /// label).
    pub name: Option<String>,
    /// The node's address path, for focus/navigation.
    pub address: Option<String>,
    /// Prebuilt name auto-assign for an UNNAMED output: the `name.some`
    /// slot address plus the next free numeric default — what the first
    /// verb naming this output applies alongside its patch write (D39).
    /// `None` when the output already has a name.
    pub name_assign: Option<(crate::ProjectSlotAddress, String)>,
    /// Ports + cells + frame — the same derivation the face bay renders.
    pub bay: UiPatchBay,
    /// The nearest enclosing module on [`UiPatchSurface::modules`], `None`
    /// when the output sits above every module face.
    pub module: Option<NodeId>,
}

impl UiPatchSurfaceOutput {
    /// What every surface row calls this output.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.label)
    }
}

/// One producing fixture: its runs (across every output) and its
/// instance grain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchSurfaceFixture {
    pub node: NodeId,
    /// The node's tree label (`dome`).
    pub label: String,
    /// The node's address path, for focus/navigation.
    pub address: Option<String>,
    /// Its runs laid along its own channel space — the fixture-side bay
    /// row, reused whole.
    pub patch: UiFixturePatch,
    /// The fixture's map2d artifact, when it has one — what a mount
    /// dispatches [`crate::AssetContentFetchOp`] against to resolve the
    /// instance table below.
    pub mapping_artifact: Option<lpc_model::ArtifactLocation>,
    /// The fixture's `{stem}.patch.json` artifact — the P6 verbs' write
    /// target. A fixture without one cannot be patched from the surface
    /// (the verbs say so rather than inventing a file).
    pub patch_artifact: Option<lpc_model::ArtifactLocation>,
    /// The map2d body was resolvable at build time. False = the fetch has
    /// not landed yet; the page dispatches it and the table fills on the
    /// next snapshot.
    pub mapping_loaded: bool,
    /// The patch body was resolvable at build time. False = the fetch has
    /// not landed yet; the page dispatches it so verbs have their document
    /// (a verb before it lands blocks honestly).
    pub patch_loaded: bool,
    /// The fixture's patch declares MANUAL flow (`flow: "manual"`, P5b):
    /// only authored entries place, and an object with no entry is
    /// genuinely unmapped — dark on the piece, no wire chip here.
    ///
    /// False = auto-mapped, the historical behaviour: the lamps no entry
    /// names flow on after the last anchor, so nothing is ever unmapped for
    /// long. Read from the patch BODY (the same bytes the verbs transform),
    /// so it is `false` while the body is still loading and while it is
    /// unreadable — silence is not the manual claim.
    pub manual_flow: bool,
    /// The addressable instance table (`/sector/0` …), parsed from the
    /// fixture's map2d document. EMPTY for shape-less strips and for docs
    /// without object ids — the fixture then patches at range grain only
    /// (the peach), which the surface renders honestly rather than
    /// inventing instances.
    pub instances: Vec<UiPatchInstance>,
    /// The fixture's Arrange-canvas facts from `editor.json`. `None` until
    /// the editor-meta fetch settles (see
    /// [`UiPatchSurface::editor_meta_loaded`]).
    pub arrange: Option<UiArrangeMeta>,
    /// The nearest enclosing module on [`UiPatchSurface::modules`], `None`
    /// when the fixture sits above every module face.
    pub module: Option<NodeId>,
}

/// One addressable node of a fixture's object tree: an instance (or a
/// whole non-repeated object), with the lamp range it currently occupies.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPatchInstance {
    /// The D46 path (`/sector/2`) — the patch entry's `from`, verbatim.
    /// EMPTY when the owning object has no sticky id yet: the strand still
    /// displays and selects (at range grain, `UiPatchTarget::Range`), but
    /// no patch entry can address it until ensure-ids stamps the document.
    pub path: String,
    /// Display label (`sector 2`).
    pub label: String,
    /// First lamp in the fixture's own numbering (derived — shown, never
    /// stored).
    pub start: u32,
    pub lamps: u32,
    /// The rotation stride the UI steps offsets by (the addressed node's
    /// shape stride — a polygon door's lamps-per-side).
    pub stride: u32,
    /// Some patch entry places this instance's lamps on a wire (derived
    /// from the fixture's resolved runs, never stored) — the tree's
    /// mapped/unmapped dot.
    pub placed: bool,
}

/// A fixture's Arrange-canvas placement facts, from the project-level
/// `editor.json` — presentation only, NEVER a sampling input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiArrangeMeta {
    /// The fixture has a `"mapping"` entry in `editor.json`. False = it has
    /// never been dragged: the canvas auto-packs it in the bottom row, and
    /// the first drag writes its entry.
    pub arranged: bool,
    /// Placement in the conceptual space (identity while unarranged).
    pub transform: UiArrangeTransform,
    /// Cached placeholder footprint (doc-space bbox + lamps) for rendering
    /// the fixture before its map2d body loads. `None` = never cached; the
    /// canvas falls back to the fixture's lamp count alone.
    pub footprint: Option<UiArrangeFootprint>,
}

/// Translate + rotate + uniform scale, no shear (ratified) — the DTO twin
/// of [`lpc_mapping::EditorTransform`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiArrangeTransform {
    /// Translation in doc-space units.
    pub t: [f64; 2],
    /// Rotation in degrees.
    pub r: f64,
    /// Uniform scale.
    pub s: f64,
}

impl Default for UiArrangeTransform {
    fn default() -> Self {
        Self {
            t: [0.0, 0.0],
            r: 0.0,
            s: 1.0,
        }
    }
}

/// The cached placeholder facts mirrored from `editor.json`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiArrangeFootprint {
    /// `[x, y, w, h]` in the fixture's own doc space.
    pub bbox: [f64; 4],
    pub lamps: u32,
}

/// What the surface's one shared selection points at (P5: highlight only;
/// P6: the verbs' subject). Stored in core ui state so e2e can drive it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPatchTarget {
    /// A module level above the fixtures/outputs (tree context row).
    Module { node: NodeId },
    /// A whole output (sidebar header).
    Output { node: NodeId },
    /// One port of an output.
    Port { node: NodeId, port: u32 },
    /// A SEGMENT: a contiguous lamp window on one port, in WIRE numbering
    /// (`start` is the output's own lamp index, inside `port`'s span).
    ///
    /// The walk-up surface's wire-side unit for FREE space — the window a
    /// click on unmapped port lamps draws, sized to the object it is about
    /// to take. A MAPPED run is NOT a segment: it keeps selecting as
    /// [`Self::Cell`], which already names it and speaks the fixture's
    /// language. WLED's word, ratified; "universe" stays banned.
    Segment {
        node: NodeId,
        port: u32,
        start: u32,
        lamps: u32,
    },
    /// One bay cell, by its twin-hover run id.
    Cell { id: String },
    /// One fixture instance, by its path.
    Instance { node: NodeId, path: String },
    /// A whole fixture.
    Fixture { node: NodeId },
    /// A fixture-relative lamp range (the range-grain fixtures' selection
    /// unit — the peach; `count: None` = to the end).
    Range {
        node: NodeId,
        start: u32,
        count: Option<u32>,
    },
}

impl UiPatchTarget {
    /// Which space this selection kind counts its lamps in — the numbering
    /// half of the surface's vocabulary (the panel's blue edge reads this
    /// too). `None` = a level that names no lamps at all.
    #[must_use]
    pub fn pulse_space(&self) -> Option<crate::PatchPulseSpace> {
        match self {
            // Fixture-side: the object's own numbering, whatever grain it
            // was named at — a mapped run included, since a cell IS a piece
            // of its fixture.
            Self::Fixture { .. }
            | Self::Instance { .. }
            | Self::Range { .. }
            | Self::Cell { .. } => Some(crate::PatchPulseSpace::Fixture),
            // Wire-side: lamps on an output, named by the wire.
            Self::Output { .. } | Self::Port { .. } | Self::Segment { .. } => {
                Some(crate::PatchPulseSpace::Wire)
            }
            // A module is a level above both spaces: it names no lamps.
            Self::Module { .. } => None,
        }
    }

    /// **THE selection-kind → language matrix (D9)**, and the only place it
    /// is decided. `None` = a selection that pulses nothing.
    ///
    /// | selection kind | space | language |
    /// |---|---|---|
    /// | [`Self::Instance`] / [`Self::Range`] / [`Self::Cell`] (incl. a mapped run) | fixture | CHASE |
    /// | [`Self::Fixture`] | fixture | BREATH |
    /// | [`Self::Output`] / [`Self::Port`] / free [`Self::Segment`] | wire | BREATH |
    /// | [`Self::Module`], nothing | — | no pulse |
    ///
    /// The reason is the question each selection asks. CHASE is the
    /// direction language: "does this OBJECT run the way I think it does?"
    /// — one strand, one head, one tail. Everything else asks "which lamps
    /// ARE this?", which white breathing answers without claiming a
    /// direction it does not have.
    ///
    /// A whole fixture used to chase, and the G1 round-3 walk rejected it: a
    /// multi-run fixture painted two heads and two tails and read as two
    /// objects selected at once. A fixture is not an object (Q8) — it is a
    /// bag of them — so it BREATHES: "these lamps are this fixture", no
    /// direction claim. See `docs/adr/2026-08-10-patch-selection-pulse.md`
    /// (Amendments 2026-08-20 and 2026-08-22).
    #[must_use]
    pub fn pulse_language(&self) -> Option<crate::PatchPulseLanguage> {
        match self {
            Self::Instance { .. } | Self::Range { .. } | Self::Cell { .. } => {
                Some(crate::PatchPulseLanguage::Chase)
            }
            Self::Fixture { .. }
            | Self::Output { .. }
            | Self::Port { .. }
            | Self::Segment { .. } => Some(crate::PatchPulseLanguage::Breath),
            Self::Module { .. } => None,
        }
    }

    /// The pulse subject for this target once the CLIENT has resolved its
    /// lamp numbers: space and language both come from the matrix above, so
    /// a surface that knows `(node, range)` never picks a tongue by hand.
    #[must_use]
    pub fn pulse_subject(
        &self,
        node: NodeId,
        range: Option<(u32, u32)>,
    ) -> Option<crate::PatchPulseSubject> {
        Some(crate::PatchPulseSubject {
            lamps: self.pulse_space()?.lamps(node, range),
            language: self.pulse_language()?,
        })
    }

    /// The sibling class this target selects in — the invariant unit of
    /// [`UiSelection`]. Fixtures are siblings at the project root; a
    /// fixture's objects are siblings within that fixture; everything else
    /// (wire-side places, cells, modules) selects alone.
    #[must_use]
    fn sibling_class(&self) -> SiblingClass {
        match self {
            Self::Fixture { .. } => SiblingClass::Root,
            Self::Instance { node, .. } | Self::Range { node, .. } => SiblingClass::Objects(*node),
            Self::Output { .. }
            | Self::Port { .. }
            | Self::Segment { .. }
            | Self::Cell { .. }
            | Self::Module { .. } => SiblingClass::Solo,
        }
    }
}

/// Which siblings a target may share a selection with (see
/// [`UiPatchTarget::sibling_class`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SiblingClass {
    /// Fixture-grain: siblings at the project root — multi across fixtures.
    Root,
    /// Object-grain within ONE fixture: siblings share the fixture.
    Objects(NodeId),
    /// Selects alone (wire-side places, cells, modules).
    Solo,
}

/// THE one selection (unified-selection D1/D2): a sibling-level SET of
/// targets plus the derived scope, one value, core-owned.
///
/// Invariants (enforced by the write-helpers — no surface restates them):
///
/// - `targets` is one sibling class: all fixture-grain (multi across
///   fixtures), or all object-grain within one fixture, or a single
///   wire-side/module target. Selecting across classes REPLACES.
/// - `entered` — the "entered fixture" the Mapping view renders as the
///   dive — is RECOMPUTED from `targets` on every write (object-grain
///   targets imply their fixture; everything else implies root), so scope
///   can never drift from selection. It survives independently only while
///   `targets` is empty: the entered-EMPTY-fixture state (`enter`), which
///   pure derivation cannot represent and drawing the first object needs.
///
/// What the value MEANS is view policy: Mapping renders `entered` as the
/// dive; Patching (no dive) reads only the targets. Cross-view the value
/// simply persists — one selection, one tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiSelection {
    entered: Option<NodeId>,
    targets: Vec<UiPatchTarget>,
}

impl UiSelection {
    /// Nothing selected, nothing entered.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Exactly one target (the every-surface single-click form).
    #[must_use]
    pub fn one(target: UiPatchTarget) -> Self {
        let mut selection = Self::empty();
        selection.select_one(target);
        selection
    }

    /// The old `Option<UiPatchTarget>` shape, for call sites that select
    /// zero-or-one.
    #[must_use]
    pub fn from_option(target: Option<UiPatchTarget>) -> Self {
        target.map(Self::one).unwrap_or_default()
    }

    /// Replace the selection with one target (scope recomputes).
    pub fn select_one(&mut self, target: UiPatchTarget) {
        self.targets = vec![target];
        self.entered = derived_entered(&self.targets);
    }

    /// Shift-click: toggle `target` within its sibling class. A different
    /// class (or a solo target) replaces the selection instead. Removing
    /// the last member leaves an empty selection at root.
    pub fn toggle_sibling(&mut self, target: UiPatchTarget) {
        let class = target.sibling_class();
        let joins = class != SiblingClass::Solo
            && self
                .targets
                .first()
                .is_some_and(|first| first.sibling_class() == class);
        if !joins {
            self.select_one(target);
            return;
        }
        if let Some(index) = self.targets.iter().position(|t| *t == target) {
            self.targets.remove(index);
        } else {
            self.targets.push(target);
        }
        self.entered = derived_entered(&self.targets);
    }

    /// Replace with a sibling SET (marquee): the first target's class
    /// keeps its siblings, everything else drops, duplicates collapse.
    pub fn set_siblings(&mut self, targets: Vec<UiPatchTarget>) {
        let Some(first) = targets.first() else {
            self.clear();
            return;
        };
        let class = first.sibling_class();
        let mut kept: Vec<UiPatchTarget> = Vec::new();
        for target in targets {
            let matches = if class == SiblingClass::Solo {
                kept.is_empty()
            } else {
                target.sibling_class() == class
            };
            if matches && !kept.contains(&target) {
                kept.push(target);
            }
        }
        self.targets = kept;
        self.entered = derived_entered(&self.targets);
    }

    /// Enter a fixture with nothing selected inside it — the empty-document
    /// drawing state, the one place scope outlives derivation.
    pub fn enter(&mut self, node: NodeId) {
        self.targets.clear();
        self.entered = Some(node);
    }

    /// Ascend one level (the esc ladder's rung, project grain): objects →
    /// their fixture selected at root; an entered-empty fixture → that
    /// fixture selected; anything else → clear. Returns whether anything
    /// changed.
    pub fn ascend(&mut self) -> bool {
        if let Some(node) = self.scope() {
            self.targets = vec![UiPatchTarget::Fixture { node }];
            self.entered = None;
            return true;
        }
        if self.targets.is_empty() {
            return false;
        }
        self.clear();
        true
    }

    /// Deselect everything (scope included — empty click's full ascend).
    pub fn clear(&mut self) {
        self.targets.clear();
        self.entered = None;
    }

    /// The entered fixture — object-grain selection's fixture, or the
    /// explicitly entered empty one. The Mapping view renders this as the
    /// dive.
    #[must_use]
    pub fn entered(&self) -> Option<NodeId> {
        self.entered
    }

    /// The scope alias for `entered` (the map2d ADR's word, lifted a
    /// level).
    #[must_use]
    pub fn scope(&self) -> Option<NodeId> {
        self.entered
    }

    #[must_use]
    pub fn targets(&self) -> &[UiPatchTarget] {
        &self.targets
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// The target, when exactly one is selected — the single-subject
    /// surfaces (verbs, arms, props) speak this.
    #[must_use]
    pub fn single(&self) -> Option<&UiPatchTarget> {
        match self.targets.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    /// The first target (multi-aware surfaces that need a representative).
    #[must_use]
    pub fn primary(&self) -> Option<&UiPatchTarget> {
        self.targets.first()
    }

    #[must_use]
    pub fn contains(&self, target: &UiPatchTarget) -> bool {
        self.targets.contains(target)
    }
}

/// The derived scope: object-grain targets imply their fixture; everything
/// else implies root. (The entered-empty case bypasses this — see
/// [`UiSelection::enter`].)
fn derived_entered(targets: &[UiPatchTarget]) -> Option<NodeId> {
    match targets.first() {
        Some(UiPatchTarget::Instance { node, .. } | UiPatchTarget::Range { node, .. }) => {
            Some(*node)
        }
        _ => None,
    }
}

/// Parse a fixture's map2d body into its instance table.
///
/// The resolver expands EVERY object — repeats and all — so the effective
/// tree always displays, whatever the document's format age (grain
/// robustness: old data must never collapse the Patch view). The sticky id
/// gates only ADDRESSABILITY: an id-bearing strand gets its D46 path
/// (`/sector/2`, what patch entries key on); an id-less strand gets an
/// empty path — it displays, selects, and pulses at range grain, and
/// becomes path-addressable the moment ensure-ids stamps the document.
pub(crate) fn instances_from_map2d(text: &str) -> Vec<UiPatchInstance> {
    let Ok(doc) = lpc_mapping::Map2dDoc::from_json(text) else {
        return Vec::new();
    };
    let Ok(resolved) = lpc_mapping::resolve(&doc) else {
        return Vec::new();
    };
    let spans = lpc_mapping::object_instance_spans(&doc, &resolved);
    spans
        .iter()
        .filter_map(|span| {
            let object = doc.objects.get(span.object)?;
            let path = span
                .id
                .as_ref()
                .map(|id| {
                    lpc_mapping::MapObjectPath {
                        id: id.clone(),
                        instances: span.instances.clone(),
                    }
                    .to_text()
                })
                .unwrap_or_default();
            // The stride of the ADDRESSED node: descend the object's shape
            // through the instance steps (a door instance's stride is the
            // polygon's side, not the repeat's whole inner count).
            let mut shape = &object.shape;
            for _ in &span.instances {
                if let lpc_mapping::Map2dShape::Repeat(repeat) = shape {
                    shape = &repeat.shape;
                }
            }
            let stride = object
                .stride
                .filter(|stride| *stride > 0)
                .unwrap_or_else(|| lpc_mapping::shape_stride(shape));
            // Display name: the id when there is one, else the authored
            // name, else an honest positional fallback.
            let base = span
                .id
                .as_ref()
                .map(|id| id.as_str().to_string())
                .filter(|base| !base.is_empty())
                .or_else(|| Some(object.name.clone()).filter(|name| !name.is_empty()))
                .unwrap_or_else(|| format!("object {}", span.object + 1));
            let label = if span.instances.is_empty() {
                base
            } else {
                format!(
                    "{} {}",
                    base,
                    span.instances
                        .iter()
                        .map(|instance| instance.to_string())
                        .collect::<Vec<_>>()
                        .join(".")
                )
            };
            Some(UiPatchInstance {
                path,
                label,
                start: span.start,
                lamps: span.count,
                stride,
                // Derived against the fixture's resolved runs at surface
                // build; parsing alone cannot know.
                placed: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The small-dome's doors document, in miniature: instances address by
    /// path, and the stride is the ADDRESSED node's (the polygon's side,
    /// not the repeat's inner count).
    #[test]
    fn instances_parse_with_the_addressed_nodes_stride() {
        let doc = r#"{
  "format": 3,
  "objects": [
    { "name": "door", "id": "door", "shape": { "repeat": {
        "shape": { "polygon": { "points": [[0.0,0.0],[12.0,0.0],[6.0,10.4]], "count": 9 } },
        "center": [60.0,60.0], "count": 3 } } }
  ]
}"#;
        let instances = instances_from_map2d(doc);
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[1].path, "/door/1");
        assert_eq!(instances[1].label, "door 1");
        assert_eq!((instances[1].start, instances[1].lamps), (9, 9));
        assert_eq!(instances[1].stride, 3, "one polygon side, not 9");
    }

    /// A document without ids still DISPLAYS its effective expansion —
    /// old-format data must never collapse the Patch tree (G1 R2, the
    /// zook lesson) — but the rows carry EMPTY paths: no address grain is
    /// invented that the patch format cannot store, so they select and
    /// pulse as ranges until ensure-ids stamps the document.
    #[test]
    fn documents_without_ids_display_but_are_not_addressable() {
        let doc = r#"{
  "format": 1,
  "objects": [
    { "name": "sector", "shape": { "repeat": {
        "shape": { "grid": { "origin": [0.5,0.5], "cols": 8, "rows": 1, "pitch": 1 } },
        "center": [60.0,60.0], "count": 5 } } }
  ]
}"#;
        let instances = instances_from_map2d(doc);
        assert_eq!(instances.len(), 5, "the repeat expands for display");
        assert_eq!(instances[2].label, "sector 2");
        assert_eq!(
            instances[2].path, "",
            "no id, no address — range grain until ensure-ids"
        );
        assert_eq!((instances[2].start, instances[2].lamps), (16, 8));
    }

    /// An explicit object-level stride override beats the derivation.
    #[test]
    fn the_authored_stride_override_wins() {
        let doc = r#"{
  "format": 3,
  "objects": [
    { "name": "sector", "id": "sector", "stride": 6, "shape": { "repeat": {
        "shape": { "path": { "points": [[0.0,0.0],[30.0,0.0]], "count": 30 } },
        "center": [60.0,60.0], "count": 5 } } }
  ]
}"#;
        let instances = instances_from_map2d(doc);
        assert_eq!(instances.len(), 5);
        assert!(instances.iter().all(|instance| instance.stride == 6));
    }

    fn fixture(node: u32) -> UiPatchTarget {
        UiPatchTarget::Fixture {
            node: NodeId::new(node),
        }
    }

    fn instance(node: u32, path: &str) -> UiPatchTarget {
        UiPatchTarget::Instance {
            node: NodeId::new(node),
            path: path.to_string(),
        }
    }

    /// Fixtures are siblings at the root: shift-click accumulates and
    /// toggles, and the scope stays root (no fixture is entered by
    /// selecting fixtures).
    #[test]
    fn fixtures_toggle_as_root_siblings() {
        let mut selection = UiSelection::one(fixture(1));
        selection.toggle_sibling(fixture(2));
        assert_eq!(selection.len(), 2);
        assert_eq!(selection.entered(), None);
        selection.toggle_sibling(fixture(1));
        assert_eq!(selection.targets(), &[fixture(2)]);
        selection.toggle_sibling(fixture(2));
        assert!(selection.is_empty());
        assert_eq!(selection.entered(), None);
    }

    /// Object-grain selection implies its fixture as the derived scope,
    /// and toggling stays within that one fixture — an object of ANOTHER
    /// fixture replaces (sibling-level only, the map2d ADR rule lifted).
    #[test]
    fn objects_are_siblings_within_one_fixture_and_imply_scope() {
        let mut selection = UiSelection::one(instance(1, "/a/0"));
        assert_eq!(selection.entered(), Some(NodeId::new(1)));
        selection.toggle_sibling(instance(1, "/a/1"));
        assert_eq!(selection.len(), 2);
        assert_eq!(selection.entered(), Some(NodeId::new(1)));
        selection.toggle_sibling(instance(2, "/b/0"));
        assert_eq!(
            selection.targets(),
            &[instance(2, "/b/0")],
            "a different fixture's object replaces, never co-selects"
        );
        assert_eq!(selection.entered(), Some(NodeId::new(2)));
    }

    /// Wire-side targets select alone: a second one replaces, and a
    /// fixture click replaces it right back.
    #[test]
    fn wire_targets_select_alone() {
        let port = UiPatchTarget::Port {
            node: NodeId::new(3),
            port: 0,
        };
        let mut selection = UiSelection::one(port.clone());
        assert_eq!(selection.entered(), None);
        selection.toggle_sibling(fixture(1));
        assert_eq!(selection.targets(), &[fixture(1)]);
        selection.toggle_sibling(port);
        assert_eq!(selection.len(), 1);
        assert!(selection.single().is_some());
    }

    /// `set_siblings` (the marquee) keeps the first target's class, drops
    /// the rest, and collapses duplicates.
    #[test]
    fn set_siblings_enforces_one_class() {
        let mut selection = UiSelection::empty();
        selection.set_siblings(vec![
            fixture(1),
            fixture(2),
            instance(1, "/a/0"),
            fixture(1),
        ]);
        assert_eq!(selection.targets(), &[fixture(1), fixture(2)]);
        assert_eq!(selection.entered(), None);
    }

    /// The ascend ladder at project grain: objects → their fixture at
    /// root → cleared; an entered-empty fixture ascends to the fixture.
    #[test]
    fn ascend_walks_objects_to_fixture_to_clear() {
        let mut selection = UiSelection::empty();
        selection.set_siblings(vec![instance(1, "/a/0"), instance(1, "/a/1")]);
        assert!(selection.ascend());
        assert_eq!(selection.targets(), &[fixture(1)]);
        assert_eq!(selection.entered(), None);
        assert!(selection.ascend());
        assert!(selection.is_empty());
        assert!(!selection.ascend(), "nothing left to ascend");

        let mut entered = UiSelection::empty();
        entered.enter(NodeId::new(4));
        assert_eq!(entered.entered(), Some(NodeId::new(4)));
        assert!(entered.ascend());
        assert_eq!(entered.targets(), &[fixture(4)]);
        assert_eq!(entered.entered(), None);
    }

    /// Any real selection write kills a retained entered-empty scope —
    /// scope never drifts from selection (the D2 no-drift property).
    #[test]
    fn selection_writes_recompute_the_entered_scope() {
        let mut selection = UiSelection::empty();
        selection.enter(NodeId::new(4));
        selection.select_one(fixture(1));
        assert_eq!(selection.entered(), None);

        selection.enter(NodeId::new(4));
        selection.toggle_sibling(instance(2, "/b/0"));
        assert_eq!(selection.entered(), Some(NodeId::new(2)));
    }
}
