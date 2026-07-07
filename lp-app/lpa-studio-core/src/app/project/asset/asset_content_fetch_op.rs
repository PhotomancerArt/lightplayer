//! Fetch op resolving an asset's effective editor content.

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Resolve (and cache) the effective editor content for `artifact` —
/// dispatched by the editor tab when its view carries no resolved content
/// yet (`UiAssetEditorTab::content == None`).
///
/// Routed like [`crate::AssetEditOp`] against `ProjectController::NODE_ID`;
/// the controller runs `ProjectController::asset_content`, which fetches the
/// base file body through the server filesystem and caches it, so the next
/// emitted view embeds the content. A no-op when the content is already
/// resolvable locally.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetContentFetchOp {
    /// The asset artifact to resolve.
    pub artifact: ArtifactLocation,
}

impl ControllerOp for AssetContentFetchOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Load asset content",
            "Read the asset's effective content for the editor.",
            ActionPriority::Tertiary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // A read on the editor's quiet-gap budget: preempts a passive
        // refresh (the user just opened the editor) but nothing else.
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_op_is_editor_foreground_class() {
        let op = AssetContentFetchOp {
            artifact: ArtifactLocation::file("/shader.glsl"),
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Load asset content");
    }
}
