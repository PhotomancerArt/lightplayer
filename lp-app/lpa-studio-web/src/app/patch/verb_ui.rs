//! The patch verbs' UI-side helpers — pure functions over `(surface,
//! selection)` shared by every surface that dispatches patch verbs (the
//! workbench Patching view; formerly the interim `/patch` page). They
//! translate the ONE core-owned selection (`UiPatchTarget`) into verb
//! subjects and `PatchVerbOp` dispatches; nothing here renders.

use dioxus::prelude::*;
use lpa_studio_core::{
    PatchVerbFixture, PatchVerbKind, PatchVerbOp, PatchVerbSubject, PatchVerbWindow,
    ProjectController, UiAction, UiPatchSurface, UiPatchSurfaceOutput, UiPatchTarget,
};

/// Every fixture's write-target facts, for the verb ops.
pub(crate) fn verb_fixtures(surface: &UiPatchSurface) -> Vec<PatchVerbFixture> {
    surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(PatchVerbFixture {
                node: fixture.node,
                patch_artifact: fixture.patch_artifact.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
                lamp_count: fixture.patch.lamps,
            })
        })
        .collect()
}

/// The verb subject for the current selection: an instance path, a cell's
/// range (or its instance, when one covers it), or the whole fixture.
pub(crate) fn verb_subject(
    surface: &UiPatchSurface,
    selection: &UiPatchTarget,
) -> Option<(lpa_studio_core::NodeId, PatchVerbSubject, u32)> {
    match selection {
        UiPatchTarget::Instance { node, path } => {
            let fixture = surface.fixtures.iter().find(|f| f.node == *node)?;
            let stride = fixture
                .instances
                .iter()
                .find(|instance| instance.path == *path)
                .map(|instance| instance.stride)
                .unwrap_or(1);
            Some((
                *node,
                PatchVerbSubject {
                    path: Some(path.clone()),
                    range: None,
                },
                stride,
            ))
        }
        UiPatchTarget::Fixture { node } => Some((
            *node,
            PatchVerbSubject {
                path: None,
                range: None,
            },
            1,
        )),
        UiPatchTarget::Cell { id } => {
            // Cell ids are `node:output:source:wire` (the bay's format).
            let node: u32 = id.split(':').next()?.parse().ok()?;
            let node = lpa_studio_core::NodeId::new(node);
            let fixture = surface.fixtures.iter().find(|f| f.node == node)?;
            let cell = fixture.patch.cells.iter().find(|cell| cell.id == *id)?;
            // Prefer the instance covering the cell — path entries match by
            // path, and the instance's stride is the honest rotation step.
            if let Some(instance) = fixture.instances.iter().find(|instance| {
                cell.source_start >= instance.start
                    && cell.source_start < instance.start + instance.lamps
            }) {
                return Some((
                    node,
                    PatchVerbSubject {
                        path: Some(instance.path.clone()),
                        range: None,
                    },
                    instance.stride,
                ));
            }
            Some((
                node,
                PatchVerbSubject {
                    path: None,
                    range: Some((cell.source_start, Some(cell.lamps))),
                },
                1,
            ))
        }
        // The range arm (P2 substrate): a fixture-relative lamp range is
        // its own verb subject — the peach's grain.
        UiPatchTarget::Range { node, start, count } => Some((
            *node,
            PatchVerbSubject {
                path: None,
                range: Some((*start, *count)),
            },
            1,
        )),
        // Wire-side and context levels are not verb subjects: the verbs act
        // on an OBJECT. A free segment is the counterpart an armed verb
        // completes against, not a subject in its own right (the arm
        // grammar lands in P3).
        UiPatchTarget::Output { .. }
        | UiPatchTarget::Port { .. }
        | UiPatchTarget::Segment { .. }
        | UiPatchTarget::Module { .. } => None,
    }
}

/// Dispatch one verb over the current selection.
pub(crate) fn dispatch_verb(
    on_action: &EventHandler<UiAction>,
    surface: &UiPatchSurface,
    selection: &Option<UiPatchTarget>,
    verb: PatchVerbKind,
) {
    let subject = selection
        .as_ref()
        .and_then(|selection| verb_subject(surface, selection));
    let (subject_fixture, subject) = match (&verb, subject) {
        // Undo/redo and port verbs need no subject.
        (
            PatchVerbKind::Undo
            | PatchVerbKind::Redo
            | PatchVerbKind::SwapPorts { .. }
            | PatchVerbKind::ShiftPort { .. },
            resolved,
        ) => (
            resolved.map(|(node, _, _)| node),
            PatchVerbSubject::default(),
        ),
        (_, Some((node, subject, _))) => (Some(node), subject),
        (_, None) => return,
    };
    on_action.call(UiAction::from_op(
        ProjectController::NODE_ID,
        PatchVerbOp {
            subject_fixture,
            subject,
            fixtures: verb_fixtures(surface),
            assign_output_name: None,
            verb,
        },
    ));
}

/// The selection's rotation stride (1 when it has none).
pub(crate) fn selection_stride(surface: &UiPatchSurface, selection: &Option<UiPatchTarget>) -> u32 {
    selection
        .as_ref()
        .and_then(|selection| verb_subject(surface, selection))
        .map(|(_, _, stride)| stride)
        .unwrap_or(1)
}

/// The next free wire lamp on a port (after its last occupied cell).
pub(crate) fn port_next_free(output: &UiPatchSurfaceOutput, port_key: u32) -> Option<u32> {
    let port = output.bay.ports.iter().find(|port| port.key == port_key)?;
    let last = port
        .cells
        .iter()
        .map(|cell| cell.wire_start + cell.lamps)
        .max()
        .unwrap_or(port.start);
    Some(last.max(port.start))
}

/// One port's swap-verb window, when the output has that port.
pub(crate) fn port_window(output: &UiPatchSurfaceOutput, key: u32) -> Option<PatchVerbWindow> {
    output
        .bay
        .ports
        .iter()
        .find(|port| port.key == key)
        .map(|port| PatchVerbWindow {
            output_name: output.name.clone(),
            start: port.start,
            lamps: port.lamps,
        })
}
