//! The selection COORDINATOR (unified-selection P5): the two-way seam
//! between the ONE core selection (`UiSelection` — project grain, D46
//! instance paths) and the dived session's `MapSelection` (document grain,
//! positional `ShapePath`s).
//!
//! The disease this kills is independent writes: before it, the tree, the
//! canvas and the Props pane each wrote whichever store they could reach,
//! and the two stores drifted. Now every surface writes its natural store
//! and the bridge RECONCILES — deterministically, change-guarded, and
//! lossy only in the ruled direction (A1: core carries the nearest
//! ADDRESSABLE projection — an object's instances — while the session
//! keeps the precise positional truth: descent, vertex, drafts).
//!
//! Direction rules:
//!
//! - **core → session** (the seed): when the scope is entered — or the
//!   core selection changes under it — the session's selected roots are
//!   set to the objects the core paths name. Keyed on the CORE facts
//!   (never the session), so in-dive session writes are never clobbered.
//! - **session → core** (the mirror): a session write projects each
//!   selected root object to ALL of its instance targets and dispatches —
//!   UNLESS the core already names the same object set at a finer grain
//!   (the enter gesture's clicked instance), in which case core stands.
//!   Id-less documents cannot be addressed (A1) and mirror nothing.

use dioxus::prelude::*;
use lpa_mapping_editor::MapEditorSession;
use lpa_studio_core::{
    NodeId, ProjectEditorOp, UiAction, UiPatchInstance, UiPatchSurface, UiPatchTarget, UiSelection,
};
use lpc_mapping::Map2dDoc;

use super::mapping_session::DiveAssetState;
use crate::app::workbench::panels::authored_object_for_instance_path;

/// Dispatch the one selection (the coordinator's single write path).
pub(crate) fn dispatch_selection(on_action: &EventHandler<UiAction>, selection: UiSelection) {
    on_action.call(UiAction::from_op(
        lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
        ProjectEditorOp::PatchSelect { selection },
    ));
}

/// Every instance target of one AUTHORED object: the D46 bridge run
/// forward — instances whose path's first segment is the object's sticky
/// id. Empty for an id-less object (A1: not addressable, select coarser).
pub(crate) fn instance_targets_for_object(
    node: NodeId,
    doc: &Map2dDoc,
    instances: &[UiPatchInstance],
    object_index: usize,
) -> Vec<UiPatchTarget> {
    let Some(id) = doc
        .objects
        .get(object_index)
        .and_then(|object| object.id.as_ref())
    else {
        return Vec::new();
    };
    instances
        .iter()
        .filter(|instance| {
            instance
                .path
                .trim_start_matches('/')
                .split('/')
                .next()
                .is_some_and(|first| first == id.as_str())
        })
        .map(|instance| crate::app::patch::verb_ui::instance_target(node, instance))
        .collect()
}

/// The object indices a core selection names on `node` (the D46 bridge
/// run backward), deduplicated in order.
pub(crate) fn core_object_indices(
    node: NodeId,
    doc: &Map2dDoc,
    selection: &UiSelection,
) -> Vec<usize> {
    let mut indices: Vec<usize> = Vec::new();
    for target in selection.targets() {
        if let UiPatchTarget::Instance { node: n, path } = target
            && *n == node
            && let Some(index) = authored_object_for_instance_path(doc, path)
            && !indices.contains(&index)
        {
            indices.push(index);
        }
    }
    indices
}

/// The session's selected ROOT objects, deduplicated in order (descended
/// paths project to their root object — A1's lossy direction).
fn session_object_indices(session: &MapEditorSession) -> Vec<usize> {
    let mut indices: Vec<usize> = Vec::new();
    for path in session.selection.paths() {
        if !indices.contains(&path.object) {
            indices.push(path.object);
        }
    }
    indices
}

/// Mount the two bridge effects for the dived session. Call once from the
/// Mapping center. `facts` carry the entered fixture's instance table.
pub(crate) fn use_selection_bridge(
    focused: Option<NodeId>,
    selection: &UiSelection,
    surface: &UiPatchSurface,
    dive_session: Signal<MapEditorSession>,
    dive_state: Signal<DiveAssetState>,
    on_action: EventHandler<UiAction>,
) {
    let instances: Vec<UiPatchInstance> = focused
        .and_then(|node| {
            surface
                .fixtures
                .iter()
                .find(|fixture| fixture.node == node)
                .map(|fixture| fixture.instances.clone())
        })
        .unwrap_or_default();

    // core → session: seed the session's selected roots from the core
    // paths. Keyed on the core facts + the pipeline's readiness — never
    // on the session, so in-dive clicks are not clobbered.
    {
        let core_paths: Vec<String> = selection
            .targets()
            .iter()
            .filter_map(|target| match target {
                UiPatchTarget::Instance { node, path } if Some(*node) == focused => {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect();
        let entered_now = focused;
        let mut session = dive_session;
        let state = dive_state;
        use_effect(use_reactive!(|(entered_now, core_paths)| {
            // Re-run when the pipeline settles a body (the doc arrives
            // after the selection names it).
            let _ready = state.read().clone();
            if entered_now.is_none() || core_paths.is_empty() {
                return;
            }
            let indices: Vec<usize> = {
                let s = session.peek();
                let mut indices: Vec<usize> = Vec::new();
                for path in &core_paths {
                    if let Some(index) = authored_object_for_instance_path(s.doc(), path)
                        && !indices.contains(&index)
                    {
                        indices.push(index);
                    }
                }
                indices
            };
            if indices.is_empty() {
                return;
            }
            let current = session_object_indices(&session.peek());
            if current != indices {
                session.write().selection.set_roots(indices);
            }
        }));
    }

    // session → core: mirror the session's selected objects up as their
    // instance targets. Tracks ONLY the session; on run it compares
    // object SETS, so a finer-grained core naming the same objects (the
    // enter gesture's clicked instance) stands untouched.
    {
        let core = selection.clone();
        let entered_now = focused;
        let session = dive_session;
        use_effect(move || {
            let session_read = session.read();
            let Some(node) = entered_now else {
                return;
            };
            let roots = session_object_indices(&session_read);
            if roots.is_empty() {
                // An empty session selection is either "not yet seeded"
                // or a deliberate entered-empty state — both are the seed
                // direction's business, never a deselect to mirror.
                return;
            }
            if core_object_indices(node, session_read.doc(), &core) == roots {
                return;
            }
            let targets: Vec<UiPatchTarget> = roots
                .iter()
                .flat_map(|index| {
                    instance_targets_for_object(node, session_read.doc(), &instances, *index)
                })
                .collect();
            drop(session_read);
            if targets.is_empty() {
                return;
            }
            let mut next = core.clone();
            next.set_siblings(targets);
            if next != core {
                dispatch_selection(&on_action, next);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Map2dDoc {
        Map2dDoc::from_json(
            r#"{"format":3,"objects":[
                {"name":"sector","id":"sector","shape":{"path":{"points":[[0.0,0.0],[10.0,0.0]],"count":6,"reversed":false,"gaps":[]}}},
                {"name":"door","id":"door","shape":{"path":{"points":[[0.0,5.0],[10.0,5.0]],"count":3,"reversed":false,"gaps":[]}}}
            ]}"#,
        )
        .expect("doc parses")
    }

    fn instance(path: &str, start: u32) -> UiPatchInstance {
        UiPatchInstance {
            path: path.to_string(),
            label: path.to_string(),
            start,
            lamps: 6,
            stride: 1,
            placed: false,
        }
    }

    /// The forward bridge: an object projects to ALL of its instances,
    /// and an id-less object projects to nothing (A1).
    #[test]
    fn objects_project_to_their_instance_targets() {
        let node = NodeId::new(2);
        let instances = vec![
            instance("/sector/0", 0),
            instance("/sector/1", 6),
            instance("/door", 12),
        ];
        let targets = instance_targets_for_object(node, &doc(), &instances, 0);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| matches!(
            target,
            UiPatchTarget::Instance { path, .. } if path.starts_with("/sector")
        )));
        let mut idless = doc();
        idless.objects[0].id = None;
        assert!(
            instance_targets_for_object(node, &idless, &instances, 0).is_empty(),
            "no sticky id = not addressable, mirror nothing"
        );
    }

    /// The backward bridge dedupes to authored object indices, so the
    /// mirror can compare object SETS and leave a finer-grained core
    /// (one clicked instance) untouched.
    #[test]
    fn core_targets_project_back_to_object_indices() {
        let node = NodeId::new(2);
        let mut selection = UiSelection::empty();
        selection.set_siblings(vec![
            UiPatchTarget::Instance {
                node,
                path: "/sector/1".to_string(),
            },
            UiPatchTarget::Instance {
                node,
                path: "/sector/0".to_string(),
            },
        ]);
        assert_eq!(core_object_indices(node, &doc(), &selection), vec![0]);
    }
}
