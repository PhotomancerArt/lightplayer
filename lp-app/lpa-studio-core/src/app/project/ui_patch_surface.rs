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

use lpc_model::NodeId;

use crate::{UiFixturePatch, UiPatchBay};

/// The whole surface, in module order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPatchSurface {
    /// Every bus-consuming output that has answered a frame probe, with its
    /// bay (ports + cells) — the sidebar's tree and the canvas's wires.
    pub outputs: Vec<UiPatchSurfaceOutput>,
    /// Every producing fixture on any of those wires, with its runs and its
    /// instance table.
    pub fixtures: Vec<UiPatchSurfaceFixture>,
}

impl UiPatchSurface {
    /// Is there anything to show? Mirrors [`UiPatchBay::is_empty`]'s
    /// honesty: a surface with no output carrying a cell says nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outputs.iter().all(|output| output.bay.is_empty())
    }
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
    /// The addressable instance table (`/sector/0` …), parsed from the
    /// fixture's map2d document. EMPTY for shape-less strips and for docs
    /// without object ids — the fixture then patches at range grain only
    /// (the peach), which the surface renders honestly rather than
    /// inventing instances.
    pub instances: Vec<UiPatchInstance>,
}

/// One addressable node of a fixture's object tree: an instance (or a
/// whole non-repeated object), with the lamp range it currently occupies.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPatchInstance {
    /// The D46 path (`/sector/2`) — the patch entry's `from`, verbatim.
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
}

/// What the surface's one shared selection points at (P5: highlight only;
/// P6: the verbs' subject). Stored in core ui state so e2e can drive it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPatchTarget {
    /// A whole output (sidebar header).
    Output { node: NodeId },
    /// One port of an output.
    Port { node: NodeId, port: u32 },
    /// One bay cell, by its twin-hover run id.
    Cell { id: String },
    /// One fixture instance, by its path.
    Instance { node: NodeId, path: String },
    /// A whole fixture.
    Fixture { node: NodeId },
}

/// Parse a fixture's map2d body into its instance table.
///
/// Only strands whose object carries a stable id are addressable —
/// nameless strands are unpatchable by path and simply absent (range-grain
/// entries still reach them). Repeat instances get one row each
/// (`/sector/2`); a plain identified object gets its whole-object row
/// (`/door`) — for a single-strand object the two coincide.
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
            let id = span.id.as_ref()?;
            let path = lpc_mapping::MapObjectPath {
                id: id.clone(),
                instances: span.instances.clone(),
            };
            // The stride of the ADDRESSED node: descend the object's shape
            // through the instance steps (a door instance's stride is the
            // polygon's side, not the repeat's whole inner count).
            let object = doc
                .objects
                .iter()
                .find(|object| object.id.as_ref() == Some(id))?;
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
            let label = if span.instances.is_empty() {
                id.as_str().to_string()
            } else {
                format!(
                    "{} {}",
                    id.as_str(),
                    span.instances
                        .iter()
                        .map(|instance| instance.to_string())
                        .collect::<Vec<_>>()
                        .join(".")
                )
            };
            Some(UiPatchInstance {
                path: path.to_text(),
                label,
                start: span.start,
                lamps: span.count,
                stride,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mini-dome's doors document, in miniature: instances address by
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

    /// A document without ids (the peach) yields NO instances — the
    /// fixture patches at range grain, and the surface must not invent an
    /// address grain the format cannot store.
    #[test]
    fn documents_without_ids_have_no_instance_grain() {
        let doc = r#"{
  "format": 1,
  "objects": [
    { "name": "strand", "shape": { "grid": { "origin": [0.5,0.5], "cols": 8, "rows": 1, "pitch": 1 } } }
  ]
}"#;
        assert!(instances_from_map2d(doc).is_empty());
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
}
