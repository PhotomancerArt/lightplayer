use crate::{ControllerId, UiError};

use super::ProjectController;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectEditorTarget {
    NodeTree,
    Node { node_id: String },
    Slot { node_id: String, slot_path: String },
    Asset { asset_id: String },
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

    pub fn slot(node_id: impl Into<String>, slot_path: impl Into<String>) -> Self {
        Self::Slot {
            node_id: node_id.into(),
            slot_path: slot_path.into(),
        }
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
            Self::Slot { node_id, slot_path } => append_dotted_path(
                root.child("node").child(node_id.clone()).child("slot"),
                slot_path,
            ),
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
        match segments.as_slice() {
            ["node_tree"] => Ok(Self::NodeTree),
            ["node", node_id] => Ok(Self::node(*node_id)),
            ["node", node_id, "slot", slot_path @ ..] if !slot_path.is_empty() => {
                Ok(Self::slot(*node_id, slot_path.join(".")))
            }
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

fn append_dotted_path(mut node_id: ControllerId, value: &str) -> ControllerId {
    for segment in value.split('.') {
        node_id = node_id.child(segment);
    }
    node_id
}

fn unsupported_project_target(node_id: &ControllerId) -> UiError {
    UiError::UnsupportedAction(format!("unknown project editor target {node_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_build_expected_node_ids() {
        assert_eq!(
            ProjectEditorTarget::node_tree().node_id().as_str(),
            "studio.project.node_tree"
        );
        assert_eq!(
            ProjectEditorTarget::node("4").node_id().as_str(),
            "studio.project.node.4"
        );
        assert_eq!(
            ProjectEditorTarget::slot("4", "brightness")
                .node_id()
                .as_str(),
            "studio.project.node.4.slot.brightness"
        );
        assert_eq!(
            ProjectEditorTarget::asset("shader_main").node_id().as_str(),
            "studio.project.asset.shader_main"
        );
        assert_eq!(
            ProjectEditorTarget::changes().node_id().as_str(),
            "studio.project.changes"
        );
        assert_eq!(
            ProjectEditorTarget::bus().node_id().as_str(),
            "studio.project.bus"
        );
    }

    #[test]
    fn parser_accepts_expected_project_targets() {
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio.project.node_tree")).unwrap(),
            ProjectEditorTarget::NodeTree
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio.project.node.4")).unwrap(),
            ProjectEditorTarget::node("4")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new(
                "studio.project.node.4.slot.palette.primary",
            ))
            .unwrap(),
            ProjectEditorTarget::slot("4", "palette.primary")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio.project.asset.shader_main"))
                .unwrap(),
            ProjectEditorTarget::asset("shader_main")
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio.project.changes")).unwrap(),
            ProjectEditorTarget::Changes
        );
        assert_eq!(
            ProjectEditorTarget::parse(&ControllerId::new("studio.project.bus")).unwrap(),
            ProjectEditorTarget::Bus
        );
    }

    #[test]
    fn parser_rejects_unknown_project_targets() {
        let error = ProjectEditorTarget::parse(&ControllerId::new("studio.project.unknown"))
            .expect_err("target should be rejected");

        assert!(matches!(error, UiError::UnsupportedAction(_)));
        assert!(error.message().contains("studio.project.unknown"));
    }

    #[test]
    fn parser_rejects_malformed_slot_target() {
        let error = ProjectEditorTarget::parse(&ControllerId::new("studio.project.node.4.slot"))
            .expect_err("target should be rejected");

        assert!(matches!(error, UiError::UnsupportedAction(_)));
        assert!(error.message().contains("studio.project.node.4.slot"));
    }
}
