//! Add-node picker data (controller-produced, pane-grammar style).

use lpc_model::NodeKind;

use crate::{ControllerId, UiAction};

use super::node_create_op::{EffectImportOp, NodeCreateOp, UiAttachTarget};
use super::node_naming::{node_kind_label, node_kind_slug};

use crate::app::home::embedded_examples;

/// Picker order: the common authoring targets first, hardware-/niche kinds
/// last. Stable — the picker never reorders. `Project` is excluded (it is
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
    /// The Effects source section: shipped effect examples, vendored by
    /// copy on selection ([`EffectImportOp`]). Rendered as its own picker
    /// section below the kind rows.
    pub effects: Vec<UiAddNodeMenuEntry>,
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

/// Build the picker for one attach site: every plain kind in
/// [`PICKER_KINDS`] order, then the **Effect** source — not a kind row but
/// a folder starter: it dispatches the same [`NodeCreateOp`] with
/// `NodeKind::Project`, which the controller expands into the effect
/// folder starter (`effects/<name>/…`, effects-are-projects ADR).
pub fn add_node_menu(attach: &UiAttachTarget) -> UiAddNodeMenu {
    let mut entries: Vec<UiAddNodeMenuEntry> = PICKER_KINDS
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
        .collect();
    entries.push(UiAddNodeMenuEntry {
        kind: NodeKind::Project,
        label: String::from("Effect"),
        icon: String::from("effect"),
        action: UiAction::from_op(
            ControllerId::new(crate::ProjectController::NODE_ID),
            NodeCreateOp {
                kind: NodeKind::Project,
                attach: attach.clone(),
            },
        )
        .with_label("New Effect")
        .with_summary("Create a new effect: a small embedded project with promoted controls."),
    });
    let effects = embedded_examples()
        .iter()
        .filter(|example| example.kind == "Effect")
        .map(|example| UiAddNodeMenuEntry {
            kind: NodeKind::Project,
            label: example.name.to_string(),
            icon: String::from("effect"),
            action: UiAction::from_op(
                ControllerId::new(crate::ProjectController::NODE_ID),
                EffectImportOp {
                    example: example.id.to_string(),
                    attach: attach.clone(),
                },
            )
            .with_label(format!("Add {}", example.name))
            .with_summary(format!(
                "Copy the {} effect into this project.",
                example.name
            )),
        })
        .collect();
    UiAddNodeMenu { entries, effects }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_offers_every_kind_plus_the_effect_source_in_stable_order() {
        let menu = add_node_menu(&UiAttachTarget::ProjectRoot);

        assert_eq!(menu.entries.len(), 11, "all plain kinds plus Effect");
        assert_eq!(menu.entries[0].kind, NodeKind::Shader);
        assert_eq!(menu.entries[0].label, "Shader");
        assert_eq!(menu.entries[0].icon, "shader");
        let effect = menu.entries.last().expect("effect entry");
        assert_eq!(effect.kind, NodeKind::Project);
        assert_eq!(effect.label, "Effect");
        assert_eq!(effect.action.meta().label, "New Effect");
        // Rebuilding yields the identical menu (stable order, stable data).
        assert_eq!(menu, add_node_menu(&UiAttachTarget::ProjectRoot));
    }

    #[test]
    fn entry_actions_dispatch_create_at_the_menu_site() {
        let playlist = UiAttachTarget::Playlist {
            node: crate::ProjectNodeAddress::parse("/demo.project/loop.playlist").unwrap(),
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
