//! Inline editor data for an editable text asset slot.

use lpc_model::ArtifactLocation;

use crate::{
    AssetContentFetchOp, AssetEditOp, ControllerId, ProjectController, UiAction, UiAssetContent,
    UiAssetContentBody, UiAssetEditorKind, UiShaderError,
};

/// Editor data embedded in an asset slot row whose def reference resolves to
/// an editable artifact (the Apply/Revert mutation target). Carried on
/// [`crate::UiSlotAsset::inline_editor`], so any asset slot anywhere in the tree
/// (a shader's `source`, a fixture mapping's SVG `source`, …) becomes an
/// inline editor without special per-node plumbing.
///
/// Produced controller-side (`ProjectController` resolves the slot's source
/// path against the owning node's def artifact) for assets whose
/// [`UiAssetEditorKind::supports_editor`] gate passes.
///
/// State projections, in the affordance model's vocabulary:
///
/// - **modified (unapplied)** is deliberately *not* here — unapplied editor
///   text is editor-local chrome only (editing-model ADR D8); the web
///   component owns it and gates the inline Apply button with it.
/// - **applied (dirty)** rides [`UiAssetContent::dirty`]; the overlay-derived
///   `Unsaved` affordance on the row/save panel does the telling.
/// - **in flight** ([`Self::in_flight`]) mirrors a buffered apply awaiting
///   its ack (the existing `Busy` projection's source).
/// - **failed** ([`Self::failure`]) carries the parked failure reason
///   (rejection, transport, or the client-side size guard), presented like a
///   failed slot edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAssetEditor {
    /// The artifact the editor edits — the Apply/Revert mutation target.
    pub artifact: ArtifactLocation,
    /// Editor family; drives the syntax mode (already gate-filtered).
    pub kind: UiAssetEditorKind,
    /// The slot's source path, for display ("blast.glsl").
    pub source: String,
    /// Effective content resolved at view build (pending buffer → overlay →
    /// cached base body); `None` until the base body fetch lands.
    pub content: Option<UiAssetContent>,
    /// True while an applied body awaits its server acknowledgement.
    pub in_flight: bool,
    /// Failure reason when the last apply parked as failed.
    pub failure: Option<String>,
    /// The owning node's error status parsed for editor display (compile
    /// errors with a best-effort source location). Positions refer to the
    /// **last applied** text — the editor's modified chip is the honesty
    /// signal when the user has typed since; positions are never remapped.
    pub shader_error: Option<UiShaderError>,
}

impl UiAssetEditor {
    /// True when the resolved content is editable text (a binary, deleted,
    /// or still-unresolved body renders read-only with no Apply).
    pub fn editable(&self) -> bool {
        matches!(
            self.content.as_ref().map(|content| &content.body),
            Some(UiAssetContentBody::Text { .. })
        )
    }

    /// The Apply action for the current editor text: dispatches
    /// [`AssetEditOp::ApplyBody`] with `text` as the full replacement body.
    /// The web component calls this from both the inline Apply button and the
    /// editor's Cmd/Ctrl+Enter path so the two gestures cannot diverge.
    pub fn apply_action(&self, text: &str) -> UiAction {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            AssetEditOp::ApplyBody {
                artifact: self.artifact.clone(),
                bytes: text.as_bytes().to_vec(),
            },
        )
        .with_summary(format!(
            "Apply the edited body of {} to the running project.",
            self.source
        ))
    }

    /// Action resolving the effective content when [`Self::content`] is
    /// `None` (fetches and caches the base file body; the refreshed view
    /// then carries the content).
    pub fn fetch_action(&self) -> UiAction {
        UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            AssetContentFetchOp {
                artifact: self.artifact.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(content: Option<UiAssetContent>) -> UiAssetEditor {
        UiAssetEditor {
            artifact: ArtifactLocation::file("/shader.glsl"),
            kind: UiAssetEditorKind::Glsl,
            source: "shader.glsl".to_string(),
            content,
            in_flight: false,
            failure: None,
            shader_error: None,
        }
    }

    #[test]
    fn apply_action_carries_the_text_as_the_body_at_the_project_controller() {
        let editor = editor(Some(UiAssetContent::from_bytes(
            b"void main() {}",
            false,
            0,
        )));
        let action = editor.apply_action("void main() { x; }");

        assert!(action.is_for_node(ProjectController::NODE_ID));
        assert_eq!(
            action.op_as::<AssetEditOp>(),
            Some(&AssetEditOp::ApplyBody {
                artifact: ArtifactLocation::file("/shader.glsl"),
                bytes: b"void main() { x; }".to_vec(),
            })
        );
    }

    #[test]
    fn editability_tracks_the_resolved_content_body() {
        assert!(
            editor(Some(UiAssetContent::from_bytes(
                b"void main() {}",
                false,
                0
            )))
            .editable()
        );
        assert!(!editor(None).editable());
        assert!(!editor(Some(UiAssetContent::from_bytes(&[0xff, 0xfe], false, 0))).editable());
    }

    #[test]
    fn fetch_action_targets_the_projects_controller_with_the_artifact() {
        let action = editor(None).fetch_action();

        assert!(action.is_for_node(ProjectController::NODE_ID));
        assert_eq!(
            action.op_as::<AssetContentFetchOp>(),
            Some(&AssetContentFetchOp {
                artifact: ArtifactLocation::file("/shader.glsl"),
            })
        );
    }

    #[test]
    fn glsl_is_the_only_inline_editor_kind_today() {
        assert!(UiAssetEditorKind::Glsl.supports_editor());
        assert!(!UiAssetEditorKind::Svg.supports_editor());
        assert!(!UiAssetEditorKind::Text.supports_editor());
        assert!(!UiAssetEditorKind::Binary.supports_editor());
    }
}
