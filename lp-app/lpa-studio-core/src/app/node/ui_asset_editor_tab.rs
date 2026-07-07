//! Editor-tab view DTO for a node's editable text asset.

use lpc_model::ArtifactLocation;

use crate::{
    AssetContentFetchOp, AssetEditOp, ControllerId, ProjectController, UiAction, UiAssetContent,
    UiAssetContentBody, UiAssetEditorKind, UiPaneAction, UiShaderError,
};

/// The node pane's "editor" tab: one editable text asset referenced by the
/// node's def, resolved to the artifact Apply/Revert mutations target.
///
/// Produced controller-side (`ProjectController` resolves the slot's source
/// path against the node's def artifact) for nodes whose def references an
/// asset whose [`UiAssetEditorKind::supports_editor_tab`] gate passes.
///
/// State projections, in the affordance model's vocabulary:
///
/// - **modified (unapplied)** is deliberately *not* here — unapplied editor
///   text is editor-local chrome only (editing-model decision); the web tab
///   threads it into [`Self::apply_pane_action`] as plain arguments.
/// - **applied (dirty)** rides [`UiAssetContent::dirty`]; the overlay-derived
///   `Unsaved` affordance on the node header/save panel does the telling.
/// - **in flight** ([`Self::in_flight`]) mirrors a buffered apply awaiting
///   its ack (the existing `Busy` projection's source).
/// - **failed** ([`Self::failure`]) carries the parked failure reason
///   (rejection, transport, or the client-side size guard), presented like a
///   failed slot edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAssetEditorTab {
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
    /// The node's error status parsed for editor display (compile errors
    /// with a best-effort source location). Positions refer to the **last
    /// applied** text — the editor's modified chip is the honesty signal
    /// when the user has typed since; positions are never remapped.
    pub shader_error: Option<UiShaderError>,
}

impl UiAssetEditorTab {
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
    /// The web editor tab calls this from both the header button and the
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

    /// The Apply header action ([`UiPaneAction`]) for the pane's actions
    /// slot. `text` and `modified` are the two editor-local inputs the web
    /// threads in (the one deliberate exception to controller-produced
    /// enablement: unapplied text never enters core state); everything else
    /// — icon, label, summary, enablement rules — stays here.
    pub fn apply_pane_action(&self, text: &str, modified: bool) -> UiPaneAction {
        let mut action = self.apply_action(text);
        if !self.editable() {
            action = action.disabled("This asset body is not editable");
        } else if !modified {
            action = action.disabled("No unapplied editor changes");
        }
        UiPaneAction::new("apply", action)
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

    fn tab(content: Option<UiAssetContent>) -> UiAssetEditorTab {
        UiAssetEditorTab {
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
    fn apply_pane_action_enables_only_on_modified_editable_text() {
        let text_tab = tab(Some(UiAssetContent::from_bytes(
            b"void main() {}",
            false,
            0,
        )));

        let enabled = text_tab.apply_pane_action("void main() { x; }", true);
        assert_eq!(enabled.icon, "apply");
        assert_eq!(enabled.label(), "Apply");
        assert!(enabled.is_enabled());
        assert!(enabled.is_primary());
        assert_eq!(
            enabled.action.op_as::<AssetEditOp>(),
            Some(&AssetEditOp::ApplyBody {
                artifact: ArtifactLocation::file("/shader.glsl"),
                bytes: b"void main() { x; }".to_vec(),
            })
        );
        assert!(enabled.action.is_for_node(ProjectController::NODE_ID));

        let unmodified = text_tab.apply_pane_action("void main() {}", false);
        assert!(!unmodified.is_enabled(), "clean editor disables Apply");
    }

    #[test]
    fn non_text_content_is_read_only_and_disables_apply() {
        let unresolved = tab(None);
        assert!(!unresolved.editable());
        assert!(!unresolved.apply_pane_action("x", true).is_enabled());

        let binary = tab(Some(UiAssetContent::from_bytes(&[0xff, 0xfe], false, 0)));
        assert!(!binary.editable());
        assert!(!binary.apply_pane_action("x", true).is_enabled());
    }

    #[test]
    fn fetch_action_targets_the_projects_controller_with_the_artifact() {
        let action = tab(None).fetch_action();

        assert!(action.is_for_node(ProjectController::NODE_ID));
        assert_eq!(
            action.op_as::<AssetContentFetchOp>(),
            Some(&AssetContentFetchOp {
                artifact: ArtifactLocation::file("/shader.glsl"),
            })
        );
    }

    #[test]
    fn glsl_is_the_only_editor_tab_kind_today() {
        assert!(UiAssetEditorKind::Glsl.supports_editor_tab());
        assert!(!UiAssetEditorKind::Svg.supports_editor_tab());
        assert!(!UiAssetEditorKind::Text.supports_editor_tab());
        assert!(!UiAssetEditorKind::Binary.supports_editor_tab());
    }
}
