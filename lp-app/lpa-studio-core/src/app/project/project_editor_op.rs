use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectEditorOp {
    Focus,
    /// Mutate a node card's core-owned UI view-state (drawers, agent
    /// collapse, mirrored composer draft) — the node arm of the CardUiState
    /// re-home. Applied synchronously in the controller, like `Focus`.
    NodeUi(crate::app::project::node_card_ui_state::NodeUiOp),
    /// Set the surface's ONE selection (D36 + unified-selection D2;
    /// core-owned so e2e can drive it and the verbs can read it). The op
    /// carries the whole [`crate::UiSelection`] VALUE — surfaces compute
    /// the next selection through its invariant-enforcing helpers and
    /// dispatch it; the controller just stores. Applied synchronously.
    PatchSelect {
        selection: crate::UiSelection,
    },
    /// Record an event into the undo-correlation journal (unified-editor
    /// P2): node/mode switches, and the mapping session's step commits
    /// observed at the shell layer (P5's glue — `lpa-mapping-editor`
    /// stays project-unaware). Applied synchronously.
    EditorJournal {
        event: crate::UiEditJournalEvent,
        node: Option<lpc_model::NodeId>,
        mode: crate::UiEditorMode,
    },
}

impl ControllerOp for ProjectEditorOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::Focus => ActionMeta::new(
                "Focus",
                "Focus this project editor surface.",
                ActionPriority::Secondary,
            ),
            Self::NodeUi(_) => ActionMeta::new(
                "Card view",
                "Change what this node card is showing.",
                ActionPriority::Tertiary,
            ),
            Self::PatchSelect { .. } => ActionMeta::new(
                "Select patch target",
                "Point the patch surface's selection.",
                ActionPriority::Tertiary,
            ),
            Self::EditorJournal { .. } => ActionMeta::new(
                "Record editor event",
                "Stamp a node/mode switch or edit into the undo journal.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Project-editor ops are foreground-class, seeded from the retired web
        // policy's `PROJECT_EDITOR_ACTION_TIMEOUT_MS` (6 s). `Focus` becomes a
        // local mutation in P3 (no network refresh), but the class it declares
        // here is the deadline the actor would apply were it to drive a pull.
        // `NodeUi` is likewise a purely local mutation: the handler never
        // awaits, so the deadline never engages.
        match self {
            Self::Focus
            | Self::NodeUi(_)
            | Self::PatchSelect { .. }
            | Self::EditorJournal { .. } => ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            },
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
    use crate::{ActionClass, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE, ProjectEditorOp};

    #[test]
    fn focus_uses_the_project_editor_deadline() {
        assert_eq!(
            ProjectEditorOp::Focus.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }

    #[test]
    fn node_ui_is_foreground_class_local_mutation() {
        let op = ProjectEditorOp::NodeUi(crate::NodeUiOp::SetAgentCollapsed {
            node: "/demo.module/orbit.shader".into(),
            collapsed: true,
        });
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }
}
