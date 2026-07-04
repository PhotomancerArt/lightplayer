//! Commands consumed by the [`StudioActor`](super::studio_actor::StudioActor).
//!
//! The actor owns the [`StudioController`](super::StudioController); every input
//! reaches it as a `StudioCommand` on an ordered queue. A user gesture becomes
//! [`StudioCommand::Action`]; the UI's refresh timer enqueues
//! [`StudioCommand::RefreshTick`] at the cadence policy's interval. Preemption is
//! therefore queue priority, not a web of cancel flags: the actor drains pending
//! actions ahead of ticks and coalesces redundant ticks (see the actor loop).

use crate::UiAction;

/// A single input to the studio actor's command queue.
#[derive(Clone, Debug)]
pub enum StudioCommand {
    /// A user-invoked action. Dispatched through the controller; its
    /// [`ActionClass`](crate::ActionClass) decides whether it preempts an
    /// in-flight passive pull.
    Action(UiAction),
    /// A timer-driven passive refresh tick. Coalescable and droppable: the actor
    /// keeps at most one pending tick and drops a tick that would run behind a
    /// pending action.
    RefreshTick,
    /// Ask the actor to finish its loop after draining nothing further. The web
    /// shell has no shutdown today, but tests use it to end the loop
    /// deterministically.
    Shutdown,
}

impl StudioCommand {
    /// Whether this command is a refresh tick (used by tick coalescing).
    pub fn is_refresh_tick(&self) -> bool {
        matches!(self, StudioCommand::RefreshTick)
    }
}
