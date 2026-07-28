//! Runtime state root for [`super::ProjectNode`].

use lpc_model::{Slotted, VisualProductSlot};

/// Runtime state exposed by a project node: the scope's `visual.out`
/// mirrored as a produced `output` (scoped-buses ADR, rule 5). The row
/// carries the project node's own product handle (playlist parity); render
/// dispatch forwards to the scope's actual producer, and a scope with no
/// visual writer renders cleared (not an error).
///
/// Engine-side on purpose — the mirror is a runtime convention, not an
/// authored artifact surface, so it stays out of the model's static shape
/// catalog (and therefore out of `schemas/`).
#[derive(Default, Slotted)]
#[slot(default_policy = "read_only_transient")]
pub struct ProjectMirrorState {
    /// This project node's own renderable handle. No `default_bind`: the
    /// mirror reads its scope's channel directly and never writes a bus
    /// channel of its own (that would make it a writer of the channel it
    /// mirrors).
    #[slot(produced)]
    pub output: VisualProductSlot,
}
