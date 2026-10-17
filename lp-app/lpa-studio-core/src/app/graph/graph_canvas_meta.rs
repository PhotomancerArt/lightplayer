//! Where the Graph's pinned nodes sit (plan Q3): `editor.json`'s per-node
//! `canvas` surface, beside the Map view's `mapping` one.
//!
//! ```jsonc
//! "nodes": { "/fixture.fixture": { "canvas": { "pin": [x, y] } } }
//! ```
//!
//! The Graph's layout is automatic; a node you pin (Debug's Pinning) stays
//! where you dropped it, in its container's coordinates. That is all the
//! surface holds. It is a surface key an older build doesn't know, which
//! `editor.json`'s reader keeps on rewrite (`lpc_mapping::editor_meta`), so
//! this needs no format bump. Nodes are keyed as the Map view keys them:
//! by their tree path.

use lab_mapping_model::{NodeId, Project, Vec2};
use lpc_mapping::EditorMetaDoc;
use serde_json::{Value, json};

/// The project's editor document, from its root.
pub(crate) const EDITOR_JSON: &str = "editor.json";
/// The Graph's surface in it.
pub(crate) const CANVAS_SURFACE: &str = "canvas";

/// Pin every node `editor.json` (`text`) says is pinned. `address_of`
/// names a lab node as `editor.json` keys it.
pub(crate) fn apply_pins(
    project: &mut Project,
    text: Option<&str>,
    address_of: impl Fn(&NodeId) -> Option<String>,
) {
    let Some(doc) = text.and_then(|t| EditorMetaDoc::from_json(t).ok()) else {
        return;
    };
    let ids: Vec<NodeId> = project.all_nodes().iter().map(|n| n.id.clone()).collect();
    for id in ids {
        let pin = address_of(&id)
            .and_then(|key| doc.nodes.get(&key))
            .and_then(|meta| meta.other_surfaces.get(CANVAS_SURFACE))
            .and_then(|canvas| canvas.get("pin"))
            .and_then(point);
        if let Some(at) = pin {
            project.pin(&id, at);
        }
    }
}

/// `editor.json` with the project's pins as its `canvas` surfaces, every
/// other surface kept; `None` when it already says exactly that (nothing
/// to write). `text` is the document as it is (`None`: there is none).
pub(crate) fn with_pins(
    text: Option<&str>,
    project: &Project,
    address_of: impl Fn(&NodeId) -> Option<String>,
) -> Option<String> {
    let mut doc = match text {
        Some(t) => EditorMetaDoc::from_json(t).ok()?,
        None => EditorMetaDoc::default(),
    };
    let before = doc.clone();
    for node in project.all_nodes() {
        let Some(key) = address_of(&node.id) else {
            continue;
        };
        let entry = doc.nodes.entry(key.clone()).or_default();
        match node.pinned {
            Some(at) => {
                entry
                    .other_surfaces
                    .insert(CANVAS_SURFACE.to_string(), json!({ "pin": [at.x, at.y] }));
            }
            None => {
                entry.other_surfaces.remove(CANVAS_SURFACE);
            }
        }
        if entry.mapping.is_none() && entry.other_surfaces.is_empty() {
            doc.nodes.remove(&key);
        }
    }
    if doc == before {
        return None;
    }
    doc.format = doc.format.max(1);
    Some(doc.to_json_pretty())
}

fn point(v: &Value) -> Option<Vec2> {
    let a = v.as_array()?;
    Some(Vec2::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pin is written as the node's `canvas` surface, the Map view's
    /// `mapping` surface is kept beside it, and today's reader reads the
    /// document back with both.
    #[test]
    fn a_pin_rides_beside_the_mapping_surface() {
        let existing = r#"{
  "format": 1,
  "nodes": {
    "/fixture.fixture": {"mapping":{"footprint":{"bbox":[0,0,10,10],"lamps":3}}}
  }
}
"#;
        let mut project = Project::seed();
        let id = project.fixtures()[0].id.clone();
        project.pin(&id, Vec2::new(12.5, 4.0));
        let address = |n: &NodeId| (n == &id).then(|| "/fixture.fixture".to_string());
        let text = with_pins(Some(existing), &project, address).expect("a pin to write");
        let doc = EditorMetaDoc::from_json(&text).expect("today's reader reads it");
        let meta = &doc.nodes["/fixture.fixture"];
        assert!(meta.mapping.is_some(), "the Map view's surface is kept");
        assert_eq!(
            meta.other_surfaces[CANVAS_SURFACE],
            json!({ "pin": [12.5, 4.0] })
        );
        assert_eq!(doc.format, 1, "no format bump");
        // Read back: the pin is where it was dropped.
        let mut reopened = Project::seed();
        apply_pins(&mut reopened, Some(&text), address);
        assert_eq!(
            reopened.node(&id).and_then(|n| n.pinned),
            Some(Vec2::new(12.5, 4.0))
        );
        // Nothing changed: nothing to write.
        assert_eq!(with_pins(Some(&text), &reopened, address), None);
    }
}
