//! Node sharing operations: copy a node to the clipboard, paste one back.
//!
//! There is no cloud provider, so the clipboard is the only way to hand
//! someone a node — and a shader is the node people most want to share.
//! Both ops move `lp.node` envelopes
//! (`docs/adr/2026-07-28-share-envelopes.md`).
//!
//! Copy is asynchronous because the def and asset bytes live on the
//! runtime's filesystem, not in the view DTOs: the controller reads them
//! over the existing `FsRead` wire path, encodes an envelope, and hands the
//! text to the clipboard through an injected sink (core stays sans-IO).

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

use super::node_create_op::UiAttachTarget;

/// Copy one node — def plus every asset it exclusively references — to the
/// clipboard as an `lp.node` envelope.
///
/// Reads the **saved** bytes off the runtime filesystem, so a node with
/// unsaved edits copies its last-saved form. The popup row says so rather
/// than silently copying stale content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeCopyOp {
    /// The node to copy.
    pub node: ProjectNodeAddress,
}

impl ControllerOp for NodeCopyOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Copy JSON",
            "Copy this node and its assets to the clipboard.",
            ActionPriority::Tertiary,
        )
        .with_icon("copy")
    }

    fn action_class(&self) -> ActionClass {
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

/// Create a node from a pasted `lp.node` envelope at `attach`.
///
/// The envelope's source paths may collide in this project, so the
/// controller re-derives free def/asset paths with the same naming rule
/// `NodeCreateOp` uses, and rewrites the def body's asset references to
/// match. Otherwise this is an ordinary `CreateNode`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodePasteOp {
    /// The raw envelope text, straight off the clipboard. Kept as text so
    /// the controller owns decoding and can report a decode failure with
    /// the envelope's own error vocabulary.
    pub envelope: String,
    /// Where the pasted node attaches.
    pub attach: UiAttachTarget,
}

impl ControllerOp for NodePasteOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Paste node",
            "Create a node from a copied node on the clipboard.",
            ActionPriority::Secondary,
        )
        .with_icon("add")
    }

    fn action_class(&self) -> ActionClass {
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
    fn share_ops_carry_the_editor_foreground_class() {
        let copy = NodeCopyOp {
            node: ProjectNodeAddress::parse("/main.project/orbit.shader").unwrap(),
        };
        let paste = NodePasteOp {
            envelope: String::new(),
            attach: UiAttachTarget::ProjectRoot,
        };
        for op in [copy.action_class(), paste.action_class()] {
            assert_eq!(
                op,
                ActionClass::Foreground {
                    deadline: PROJECT_EDITOR_ACTION_DEADLINE,
                }
            );
        }
    }

    #[test]
    fn ops_compare_by_value_not_identity() {
        let node = ProjectNodeAddress::parse("/main.project/orbit.shader").unwrap();
        let a = NodeCopyOp { node: node.clone() };
        let b = NodeCopyOp { node };
        assert!(a.eq_op(&b));
        assert!(!a.eq_op(&NodePasteOp {
            envelope: String::new(),
            attach: UiAttachTarget::ProjectRoot,
        }));
    }
}
