//! The Arrange canvas's ops over the project-level `editor.json`
//! (unified-editor P2) — DTO-prebuilt like [`crate::PatchVerbOp`], so the
//! web never imports kernel types.
//!
//! [`EditorMetaOp`] is the write path: the controller loads the cached
//! `editor.json` (an absent file reads as the empty document and the first
//! write creates it), applies the transform through
//! [`lpc_mapping::EditorMetaDoc`], refreshes stale cached footprints for
//! every fixture whose map2d body is locally cached, round-trips to the
//! canonical bytes, snapshots BOTH sides onto the arrange byte stack, and
//! writes via the normal `ApplyBody` overlay path. A document the kernel
//! refuses (newer format, parse error) blocks the write —
//! refuse-don't-rewrite, like every document editor here.
//!
//! [`EditorMetaFetchOp`] is the read path: because `editor.json` is
//! optional (most projects have never been arranged), the fetch first
//! LISTS the project root — a missing file settles as "absent" (the empty
//! document) rather than surfacing a read error, and a present file
//! fetches through the normal asset-content path.

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    UiArrangeTransform,
};

/// Where the project-level editor document lives: sibling of
/// `project.json`, one per project.
pub const EDITOR_META_PATH: &str = "/editor.json";

/// The `editor.json` artifact location (project-relative, like every
/// asset artifact).
#[must_use]
pub fn editor_meta_artifact() -> ArtifactLocation {
    ArtifactLocation::file(EDITOR_META_PATH)
}

/// One fixture's footprint-refresh facts, as the surface DTO knows them:
/// enough for the controller to recompute the cached footprint when the
/// fixture's map2d body is locally cached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorMetaFixture {
    /// The fixture's `editor.json` key — its authored address path (the
    /// persistent identity; runtime [`lpc_model::NodeId`]s never persist).
    pub node_key: String,
    /// The fixture's map2d artifact, when it has one.
    pub mapping_artifact: Option<ArtifactLocation>,
}

/// One fixture's transform write inside a (possibly multi-fixture)
/// arrange gesture.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorMetaSet {
    pub node_key: String,
    /// The subject node's runtime id, for the undo stamp.
    pub node: Option<lpc_model::NodeId>,
    pub transform: UiArrangeTransform,
}

/// What the gesture asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorMetaVerb {
    /// Set `node_key`'s `"mapping"` transform (one drag/rotate/scale
    /// gesture = one call = one undo step).
    Set {
        node_key: String,
        /// The subject node's runtime id, for the undo stamp.
        node: Option<lpc_model::NodeId>,
        transform: UiArrangeTransform,
    },
    /// Set SEVERAL fixtures' transforms as ONE write and ONE undo step —
    /// the multi-selection's move/scale gesture (unified-selection P3).
    /// One document round-trip, one byte-stack snapshot: ⌘Z restores the
    /// whole set.
    SetMany {
        entries: Vec<EditorMetaSet>,
    },
    /// Undo / redo the arrange byte stack (mode-scoped ⌘Z).
    Undo,
    Redo,
}

/// The op: routed to the project controller like [`crate::PatchVerbOp`].
#[derive(Clone, Debug, PartialEq)]
pub struct EditorMetaOp {
    /// The `editor.json` artifact ([`editor_meta_artifact`], DTO-carried).
    pub artifact: ArtifactLocation,
    /// Every fixture whose cached map2d can refresh a stale footprint as
    /// part of this write (never an unprompted write).
    pub fixtures: Vec<EditorMetaFixture>,
    pub verb: EditorMetaVerb,
}

impl ControllerOp for EditorMetaOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Arrange fixture",
            "Place a fixture on the project's arrange canvas.",
            ActionPriority::Secondary,
        )
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

/// Settle the `editor.json` loaded-flag: list the project root, fetch the
/// file when present, record "absent" when not. Pages dispatch this while
/// [`crate::UiPatchSurface::editor_meta_loaded`] is false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorMetaFetchOp {
    pub artifact: ArtifactLocation,
}

impl ControllerOp for EditorMetaFetchOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Load editor layout",
            "Fetch the project's editor.json arrangement document.",
            ActionPriority::Tertiary,
        )
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
