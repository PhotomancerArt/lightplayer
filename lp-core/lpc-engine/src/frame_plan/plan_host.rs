//! What a [`super::PlanSlot`] needs from whoever holds it: the mode, the
//! frame, the counters, and a name for its messages.

use lpc_model::NodeId;

use super::{FramePlanCounters, PlanMode};

/// The engine's plan state for the current frame: owned by the resolver
/// (which has the frame's lifetime and is threaded to every node through its
/// context), reset in `begin_frame`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramePlanState {
    pub mode: PlanMode,
    /// Frames begun since the engine was made; [`PlanMode::Check`] compares
    /// on multiples of its `every`.
    pub frame: u32,
    pub counters: FramePlanCounters,
}

impl FramePlanState {
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.counters = FramePlanCounters::default();
    }
}

/// Access to the frame's [`FramePlanState`] for a [`super::PlanSlot`]. A
/// node's `TickContext` implements it, and so does the engine for its own
/// plans; a plan's build closure receives the host back, so building may
/// resolve through it.
pub trait PlanHost {
    fn plan_state(&mut self) -> &mut FramePlanState;

    /// Who holds the plan, for a mismatch message. `None` for the engine's
    /// own plans.
    fn plan_owner(&self) -> Option<NodeId>;
}

impl PlanHost for FramePlanState {
    fn plan_state(&mut self) -> &mut FramePlanState {
        self
    }

    fn plan_owner(&self) -> Option<NodeId> {
        None
    }
}
