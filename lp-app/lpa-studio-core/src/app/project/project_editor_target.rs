use crate::{ControllerId, UiError};

use super::project_target_encoding::{
    DecodedProjectTarget, decode_typed_project_target, node_target_id, slot_target_id,
};
use super::{ProjectController, ProjectNodeTarget, ProjectSlotAddress};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectEditorTarget {
    NodeTree,
    /// Legacy node target used by the current pre-M3 project workspace.
    Node {
        node_id: String,
    },
    /// Typed node target used by the reconciled project editor model.
    AddressedNode {
        target: ProjectNodeTarget,
    },
    /// Legacy slot target used by the current pre-M3 project workspace.
    Slot {
        node_id: String,
        slot_path: String,
    },
    /// Typed slot target used by the reconciled project editor model.
    AddressedSlot {
        target: ProjectNodeTarget,
        slot: ProjectSlotAddress,
    },
    Asset {
        asset_id: String,
    },
    Changes,
    Bus,
}

impl ProjectEditorTarget {
    pub fn node_tree() -> Self {
        Self::NodeTree
    }

    pub fn node(node_id: impl Into<String>) -> Self {
        Self::Node {
            node_id: node_id.into(),
        }
    }

    pub fn addressed_node(target: ProjectNodeTarget) -> Self {
        Self::AddressedNode { target }
    }

    pub fn slot(node_id: impl Into<String>, slot_path: impl Into<String>) -> Self {
        Self::Slot {
            node_id: node_id.into(),
            slot_path: slot_path.into(),
        }
    }

    pub fn addressed_slot(target: ProjectNodeTarget, slot: ProjectSlotAddress) -> Self {
        Self::AddressedSlot { target, slot }
    }

    pub fn asset(asset_id: impl Into<String>) -> Self {
        Self::Asset {
            asset_id: asset_id.into(),
        }
    }

    pub fn changes() -> Self {
        Self::Changes
    }

    pub fn bus() -> Self {
        Self::Bus
    }

    pub fn node_id(&self) -> ControllerId {
        let root = project_node_id();
        match self {
            Self::NodeTree => root.child("node_tree"),
            Self::Node { node_id } => root.child("node").child(node_id.clone()),
            Self::AddressedNode { target } => node_target_id(&root, target),
            Self::Slot { node_id, slot_path } => root
                .child("node")
                .child(node_id.clone())
                .child("slot")
                .child(slot_path.clone()),
            Self::AddressedSlot { target, slot } => slot_target_id(&root, target, slot),
            Self::Asset { asset_id } => root.child("asset").child(asset_id.clone()),
            Self::Changes => root.child("changes"),
            Self::Bus => root.child("bus"),
        }
    }

    pub fn parse(node_id: &ControllerId) -> Result<Self, UiError> {
        let root = project_node_id();
        let Some(tail) = node_id.strip_prefix(&root) else {
            return Err(unsupported_project_target(node_id));
        };
        let segments = tail.iter().collect::<Vec<_>>();
        if let Some(target) = decode_typed_project_target(&segments)? {
            return Ok(match target {
                DecodedProjectTarget::Node(target) => Self::addressed_node(target),
                DecodedProjectTarget::Slot { node, slot } => Self::addressed_slot(node, slot),
            });
        }
        match segments.as_slice() {
            ["node_tree"] => Ok(Self::NodeTree),
            ["node", node_id] => Ok(Self::node(*node_id)),
            ["node", node_id, "slot", slot_path] => Ok(Self::slot(*node_id, *slot_path)),
            ["asset", asset_id] => Ok(Self::asset(*asset_id)),
            ["changes"] => Ok(Self::Changes),
            ["bus"] => Ok(Self::Bus),
            _ => Err(unsupported_project_target(node_id)),
        }
    }
}

fn project_node_id() -> ControllerId {
    ControllerId::new(ProjectController::NODE_ID)
}

fn unsupported_project_target(node_id: &ControllerId) -> UiError {
    UiError::UnsupportedAction(format!("unknown project editor target {node_id}"))
}

#[cfg(test)]
mod tests {
    use lpc_model::{NodeId, SlotPath};

    use super::*;
    use crate::{ProjectNodeAddress, ProjectSlotRoot};

    #[test]
    fn constructors_build_expected_node_ids() {
        assert_eq!(
            ProjectEditorTarget::node_tree().node_id().as_str(),
            "studio|project|node_tree"
        );
        assert_eq!(
            ProjectEditorTarget::node("4").node_id().as_str(),
            "studio|project|node|4"
        );
        assert_eq!(
            ProjectEditorTarget::slot("4", "brightness")
                .node_id()
                .as_str(),
            "studio|project|node|4|slot|brightness"
        );
        assert_eq!(
            ProjectEditorTarget::addressed_node(node_target())
                .node_id()
                .as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader"
        );
        assert_eq!(
            ProjectEditorTarget::addressed_slot(node_target(), slot_address())
                .node_id()
                .as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|path|config.brightness"
        );
        assert_eq!(
            ProjectEditorTarget::asset("shader_main").node_id().as_str(),
            "studio|project|asset|shader_main"
        );
        assert_eq!(
            ProjectEditorTarget::changes().node_id().as_str(),
            "studio|project|changes"
        );
        assert_eq!(
            ProjectEditorTarget::bus().node_id().as_str(),
            "studio|project|bus"
        );
    }

    #[test]
    fn parser_accepts_expected_project_targets() {
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio|project|node_tree")).unwrap(),
            ProjectEditorTarget::NodeTree
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio|project|node|4")).unwrap(),
            ProjectEditorTarget::node("4")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new(
                "studio|project|node|4|slot|palette.primary",
            ))
            .unwrap(),
            ProjectEditorTarget::slot("4", "palette.primary")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio|project|asset|shader_main"))
                .unwrap(),
            ProjectEditorTarget::asset("shader_main")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio|project|changes")).unwrap(),
            ProjectEditorTarget::Changes
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio|project|bus")).unwrap(),
            ProjectEditorTarget::Bus
        );
    }

    #[test]
    fn parser_accepts_typed_project_targets() {
        assert_eq!(
            ProjectEditorTarget::parse(
                &ProjectEditorTarget::addressed_node(node_target()).node_id()
            )
            .unwrap(),
            ProjectEditorTarget::addressed_node(node_target())
        );
        assert_eq!(
            ProjectEditorTarget::parse(
                &ProjectEditorTarget::addressed_slot(node_target(), slot_address()).node_id()
            )
            .unwrap(),
            ProjectEditorTarget::addressed_slot(node_target(), slot_address())
        );
    }

    #[test]
    fn parser_rejects_unknown_project_targets() {
        let error = ProjectEditorTarget::parse(&ControllerId::new("studio|project|unknown"))
            .expect_err("target should be rejected");

        assert!(matches!(error, UiError::UnsupportedAction(_)));
        assert!(error.message().contains("studio|project|unknown"));
    }

    #[test]
    fn parser_rejects_malformed_slot_target() {
        let error = ProjectEditorTarget::parse(&ControllerId::new("studio|project|node|4|slot"))
            .expect_err("target should be rejected");

        assert!(matches!(error, UiError::UnsupportedAction(_)));
        assert!(error.message().contains("studio|project|node|4|slot"));
    }

    fn node_target() -> ProjectNodeTarget {
        ProjectNodeTarget::new(
            ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap(),
            NodeId::new(3),
        )
    }

    fn slot_address() -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/orbit.shader").unwrap(),
            ProjectSlotRoot::def(),
            SlotPath::parse("config.brightness").unwrap(),
        )
    }
}
