//! Commands consumed by the [`StudioActor`](super::studio_actor::StudioActor).
//!
//! The actor owns the [`StudioController`](super::StudioController); every input
//! reaches it as a `StudioCommand` on an ordered queue. A user gesture becomes
//! [`StudioCommand::Action`]; the UI's refresh timer enqueues
//! [`StudioCommand::RefreshTick`] at the cadence policy's interval. Preemption is
//! therefore queue priority, not a web of cancel flags: the actor drains pending
//! actions ahead of ticks and coalesces redundant ticks (see the actor loop).

use std::rc::Rc;

use crate::UiAction;
use crate::app::library::LibraryHost;
use crate::app::studio::console_command::ConsoleCommand;

/// The injected library host riding the command queue (Debug-opaque: a
/// platform edge object).
#[derive(Clone)]
pub struct LibraryAttachment(pub Rc<dyn LibraryHost>);

impl core::fmt::Debug for LibraryAttachment {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LibraryAttachment(..)")
    }
}

/// A single input to the studio actor's command queue.
#[derive(Clone, Debug)]
pub enum StudioCommand {
    /// Attach the mounted local library (sent by the platform shell once
    /// the store is ready, before any project action). Applied
    /// synchronously by the actor ahead of the batch's actions.
    AttachLibrary(LibraryAttachment),
    /// A user-invoked action. Dispatched through the controller; its
    /// [`ActionClass`](crate::ActionClass) decides whether it preempts an
    /// in-flight passive pull.
    Action(UiAction),
    /// A console mutation (filter change or clear). Applied synchronously by
    /// the actor ahead of the batch's actions; never coalesced away, unlike
    /// `RefreshTick`, because each is a distinct user gesture.
    Console(ConsoleCommand),
    /// The library changed under us (another tab's catalog transaction or
    /// save, via the host's BroadcastChannel). Coalescable like
    /// `RefreshTick`: the actor schedules one gallery re-hydration.
    LibraryChanged,
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
