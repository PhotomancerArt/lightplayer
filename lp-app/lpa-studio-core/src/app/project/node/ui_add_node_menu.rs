//! Add-node picker data (controller-produced, pane-grammar style).

use lpc_model::NodeKind;

use crate::{ControllerId, UiAction};

use super::node_create_op::{NodeCreateOp, UiAttachTarget};
use super::node_naming::{node_kind_label, node_kind_slug};

/// Picker order: the common authoring targets first, hardware-/niche kinds
/// last. Stable — the picker never reorders. `Module` is excluded (it is
/// the artifact root; nested sub-projects are future work).
const PICKER_KINDS: &[NodeKind] = &[
    NodeKind::Shader,
    NodeKind::Texture,
    NodeKind::Playlist,
    NodeKind::Clock,
    NodeKind::Fixture,
    NodeKind::Output,
    NodeKind::Fluid,
    NodeKind::ComputeShader,
    NodeKind::Button,
    NodeKind::ControlRadio,
];

/// The add-node picker's data: one entry per instantiable kind, in stable
/// order. Exposed on [`crate::ProjectEditorView`] (project pane "+", attach
/// = project root) and on a playlist card's [`crate::UiNodeView`] (strip
/// "+", attach = that playlist).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenu {
    pub entries: Vec<UiAddNodeMenuEntry>,
    /// Where this menu's creates attach. Carried alongside the entries so
    /// the picker can offer sources the controller cannot pre-build an
    /// action for — paste needs the clipboard's contents, which only the
    /// browser edge can read (`docs/adr/2026-07-28-share-envelopes.md`).
    pub attach: UiAttachTarget,
}

/// One picker entry. `action` is the ready-to-dispatch create (pane grammar:
/// actions are controller-produced data; the renderer never assembles ops).
#[derive(Clone, Debug, PartialEq)]
pub struct UiAddNodeMenuEntry {
    pub kind: NodeKind,
    /// Human-readable kind label ("Shader", "Compute shader", …).
    pub label: String,
    /// Icon token for the renderer (the kind's name slug).
    pub icon: String,
    /// Dispatches [`NodeCreateOp`] for this kind at the menu's attach site.
    pub action: UiAction,
}

/// Build the picker for one attach site: every kind except `Module`, in
/// [`PICKER_KINDS`] order.
pub fn add_node_menu(attach: &UiAttachTarget) -> UiAddNodeMenu {
    UiAddNodeMenu {
        attach: attach.clone(),
        entries: PICKER_KINDS
            .iter()
            .map(|kind| {
                let label = node_kind_label(*kind);
                UiAddNodeMenuEntry {
                    kind: *kind,
                    label: label.to_string(),
                    icon: node_kind_slug(*kind).to_string(),
                    action: UiAction::from_op(
                        ControllerId::new(crate::ProjectController::NODE_ID),
                        NodeCreateOp {
                            kind: *kind,
                            attach: attach.clone(),
                        },
                    )
                    .with_label(format!("Add {label}"))
                    .with_summary(format!("Create a new {} node.", label.to_lowercase())),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_offers_every_kind_except_module_in_stable_order() {
        let menu = add_node_menu(&UiAttachTarget::ProjectRoot);

        assert_eq!(menu.entries.len(), 10, "all kinds except Module");
        assert!(menu.entries.iter().all(|e| e.kind != NodeKind::Module));
        assert_eq!(menu.entries[0].kind, NodeKind::Shader);
        assert_eq!(menu.entries[0].label, "Shader");
        assert_eq!(menu.entries[0].icon, "shader");
        // Rebuilding yields the identical menu (stable order, stable data).
        assert_eq!(menu, add_node_menu(&UiAttachTarget::ProjectRoot));
    }

    #[test]
    fn entry_actions_dispatch_create_at_the_menu_site() {
        let playlist = UiAttachTarget::Playlist {
            node: crate::ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap(),
        };
        let menu = add_node_menu(&playlist);
        let entry = &menu.entries[0];

        assert!(entry.action.is_for_node(crate::ProjectController::NODE_ID));
        let op = entry.action.op_as::<NodeCreateOp>().expect("create op");
        assert_eq!(op.kind, NodeKind::Shader);
        assert_eq!(op.attach, playlist);
        assert_eq!(entry.action.meta().label, "Add Shader");
    }
}
