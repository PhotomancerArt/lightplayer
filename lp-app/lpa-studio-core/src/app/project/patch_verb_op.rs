//! The patch surface's verb op (D42): one action per gesture, carrying the
//! surface facts the controller needs — the DTO-prebuilt convention, so
//! the web never imports kernel types.
//!
//! Routed like [`crate::AssetEditOp`] (against the project controller's
//! node id, executed with server access): the controller loads the cached
//! patch/map2d bodies, applies the pure transform
//! ([`super::patch_verbs`]), round-trips it through the kernel, snapshots
//! the prior bytes onto the session-local undo stack, and writes via the
//! normal `ApplyBody` path. A verb that would produce a refused document
//! reports the kernel's error and writes nothing.

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// One fixture's write-target facts, as the surface DTO knows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchVerbFixture {
    pub node: lpc_model::NodeId,
    /// The `{stem}.patch.json` artifact — the write target.
    pub patch_artifact: ArtifactLocation,
    /// The fixture's map2d artifact, for span lowering (absent = shape-less
    /// strip: range grain only).
    pub mapping_artifact: Option<ArtifactLocation>,
    /// The fixture's lamp count (validation universe).
    pub lamp_count: u32,
}

/// What the gesture asks for. Subjects are the surface's own vocabulary
/// (paths and ranges as plain data); the controller re-parses them into
/// kernel types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchVerbKind {
    /// Put the subject on `output_name` at wire `lamp` (creating its entry
    /// when absent). `output_name: None` = the default output.
    Assign {
        output_name: Option<String>,
        lamp: u32,
    },
    /// Move the subject's anchor to wire `lamp`, same output.
    ReAnchor {
        lamp: u32,
    },
    /// Toggle the subject's `reversed`.
    Reverse,
    /// Step the subject's rotation offset by `steps × stride` lamps.
    Rotate {
        steps: i32,
        stride: u32,
    },
    /// Swap the contents of two port windows (possibly across outputs),
    /// over EVERY fixture in [`PatchVerbOp::fixtures`].
    SwapPorts {
        a: PatchVerbWindow,
        b: PatchVerbWindow,
    },
    /// Shift every entry in the window by `delta` lamps, over every
    /// fixture.
    ShiftPort {
        window: PatchVerbWindow,
        delta: i32,
    },
    /// Remove the subject's entries (fixture subject = clear the doc).
    Clear,
    /// Run ensure-ids on the SUBJECT fixture's map2d document — the one
    /// mapping write this surface may make (undoable like every verb).
    EnsureIds,
    /// Undo / redo the session-local verb stack.
    Undo,
    Redo,
}

/// A port window in surface terms: output NAME (`None` = default) + wire
/// lamp span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatchVerbWindow {
    pub output_name: Option<String>,
    pub start: u32,
    pub lamps: u32,
}

/// The subject, as plain data (`path` XOR `range`; neither = the whole
/// fixture).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PatchVerbSubject {
    /// A `/sector/2`-style object path.
    pub path: Option<String>,
    /// A fixture-relative `(start, count)` range (`count: None` = to end).
    pub range: Option<(u32, Option<u32>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatchVerbOp {
    /// The subject fixture (ignored by Undo/Redo; port verbs use it as the
    /// error anchor only).
    pub subject_fixture: Option<lpc_model::NodeId>,
    pub subject: PatchVerbSubject,
    /// Every fixture the verb may touch (port verbs sweep them all;
    /// subject verbs use the one matching `subject_fixture`).
    pub fixtures: Vec<PatchVerbFixture>,
    /// Prebuilt output-name auto-assign: `(slot address text, name)` —
    /// applied as EnsurePresent + SetValue alongside the patch write when
    /// the gesture targets an output that has no name yet.
    pub assign_output_name: Option<(crate::ProjectSlotAddress, String)>,
    pub verb: PatchVerbKind,
}

impl ControllerOp for PatchVerbOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Edit patch",
            "Apply a patch-surface verb to the fixture's patch document.",
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
