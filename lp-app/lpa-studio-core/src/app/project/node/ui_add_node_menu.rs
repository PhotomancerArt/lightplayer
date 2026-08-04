//! Add-node picker data (controller-produced, pane-grammar style).

use lpc_model::{LpFeature, NodeKind};

use crate::{ControllerId, UiAction};

use super::node_create_op::{NodeCreateOp, UiAttachTarget};
use super::node_naming::{node_kind_label, node_kind_slug};

/// Picker order: the common authoring targets first, hardware-/niche kinds
/// last. Stable — the picker never reorders. `Module` sits with the other
/// container (settled D-C: an empty module is creatable, and everything
/// composable should be authorable).
const PICKER_KINDS: &[NodeKind] = &[
    NodeKind::Shader,
    NodeKind::Texture,
    NodeKind::Playlist,
    NodeKind::Module,
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
    /// Why this entry is unavailable, when it is — the connected device's
    /// firmware carries no runtime for the kind. `None` = offer it.
    ///
    /// Unavailable kinds are DISABLED, never hidden: a picker that silently
    /// drops entries teaches the wrong catalog, and "why can't I add a
    /// Fluid?" has no answer if the row is not there to carry one.
    pub unavailable: Option<String>,
}

/// Build the picker for one attach site: every instantiable kind, in
/// [`PICKER_KINDS`] order, with every entry enabled.
///
/// The device gate is applied afterwards by [`gate_add_node_menu`], once,
/// where the lens session is known — menus are built in several places and
/// only one of them can see the device.
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
                    unavailable: None,
                }
            })
            .collect(),
    }
}

/// Disable the entries the connected device cannot run.
///
/// `device_features` is what that device's hello reported. **`None` means
/// "no device has said otherwise" (a sim/host lens, or a link that is not
/// Ready yet) and everything stays enabled** — gating only ever narrows
/// when a real device affirmatively reports its build. Idempotent, and it
/// never re-enables an entry.
pub fn gate_add_node_menu(menu: &mut UiAddNodeMenu, device_features: Option<&[LpFeature]>) {
    let Some(features) = device_features else {
        return;
    };
    for entry in &mut menu.entries {
        if entry.unavailable.is_none() && kind_is_missing(entry.kind, features) {
            entry.unavailable = Some(UNAVAILABLE_COPY.to_string());
        }
    }
}

/// Picker annotation for a kind the device's firmware does not carry — the
/// same "Not on this device" family the tree status uses.
const UNAVAILABLE_COPY: &str = "Not on this device";

/// Whether the device's reported features lack this kind's runtime. Ungated
/// kinds ([`LpFeature::for_node_kind`] → `None`) are always available.
fn kind_is_missing(kind: NodeKind, features: &[LpFeature]) -> bool {
    LpFeature::for_node_kind(kind).is_some_and(|required| !features.contains(&required))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_offers_every_kind_in_stable_order() {
        let menu = add_node_menu(&UiAttachTarget::ProjectRoot);

        assert_eq!(menu.entries.len(), 11, "every instantiable kind");
        assert!(menu.entries.iter().any(|e| e.kind == NodeKind::Module));
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

    /// A device that reports its build disables exactly the kinds it lacks
    /// — and never removes an entry.
    #[test]
    fn a_reporting_device_disables_only_the_kinds_it_lacks() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        let before = menu.entries.len();
        // A build with everything except the fluid and radio runtimes.
        let features = [
            LpFeature::NodeButton,
            LpFeature::NodeClock,
            LpFeature::NodeFixture,
            LpFeature::NodePlaylist,
            LpFeature::NodeShader,
            LpFeature::NodeTexture,
            LpFeature::GfxLpvm,
        ];
        gate_add_node_menu(&mut menu, Some(&features));

        assert_eq!(
            menu.entries.len(),
            before,
            "entries are disabled, never hidden"
        );
        let disabled: Vec<NodeKind> = menu
            .entries
            .iter()
            .filter(|entry| entry.unavailable.is_some())
            .map(|entry| entry.kind)
            .collect();
        assert_eq!(disabled, vec![NodeKind::Fluid, NodeKind::ControlRadio]);
        // Output is ungated in the engine, so it survives any feature set.
        let output = menu
            .entries
            .iter()
            .find(|entry| entry.kind == NodeKind::Output)
            .expect("output entry");
        assert_eq!(output.unavailable, None);
        // Shader and ComputeShader share one gate; both stay offered.
        assert!(
            menu.entries
                .iter()
                .filter(|entry| matches!(entry.kind, NodeKind::Shader | NodeKind::ComputeShader))
                .all(|entry| entry.unavailable.is_none())
        );
    }

    /// No device has reported: nothing is gated. A sim lens must never be
    /// narrowed by a device that is not there.
    #[test]
    fn an_unknown_device_gates_nothing() {
        let mut menu = add_node_menu(&UiAttachTarget::ProjectRoot);
        gate_add_node_menu(&mut menu, None);
        assert!(menu.entries.iter().all(|entry| entry.unavailable.is_none()));
    }
}
