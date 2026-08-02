//! Runtime state root for [`super::ModuleNode`].

use lpc_model::{Slotted, VisualProductSlot};

/// Runtime state exposed by a module node: the scope's `visual.out`
/// mirrored as a produced `output` (modules.md R7). The row carries the
/// module node's own product handle (playlist parity); render dispatch
/// forwards to the scope's actual producer, and a scope with no visual
/// writer renders cleared (a module without a visual is a legitimate
/// shape, not an error).
///
/// Engine-side on purpose — the mirror is a runtime convention, not an
/// authored artifact surface, so it stays out of the model's static shape
/// catalog (and therefore out of `schemas/`).
#[derive(Default, Slotted)]
pub struct ModuleMirrorState {
    /// This module node's own renderable handle. No `default_bind`: the
    /// mirror reads its scope's channel directly and never writes a bus
    /// channel of its own (that would make it a writer of the channel it
    /// mirrors); the outward `visual.out` publish is a loader-registered
    /// binding on this produced slot (R7), not part of the mirror.
    #[slot(produced)]
    pub output: VisualProductSlot,
}
