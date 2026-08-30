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
use crate::app::agent::AgentFeedback;
use crate::app::library::LibraryHost;
use crate::app::settings::SettingsCommand;
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
    /// A settings mutation or layer load (the shell's settings popover, the
    /// boot `dev-settings.json` fetch). Applied synchronously by the actor
    /// ahead of the batch's actions, in queue order, like `Console`.
    Settings(SettingsCommand),
    /// Progress from a spawned agent run (streamed events, run end). Applied
    /// synchronously by the actor in queue order, like `Console` — each
    /// message mutates the agent session mirror and marks the view dirty.
    Agent(AgentFeedback),
    /// One input for the device model, from the effects layer: a link event, a
    /// timer that fired, a port that appeared or left.
    ///
    /// This is how invariant I7 is kept: device IO happens in spawned futures
    /// that end HERE, on the same ordered queue a click arrives on, and the
    /// actor's fold of it is synchronous. Applied in queue order and never
    /// coalesced — an event stream's order IS its meaning.
    Device(lpa_devices::Input),
    /// A `navigator.serial` hotplug edge. Not a model input: it makes the
    /// effects layer go looking, and what it finds becomes `Device` commands.
    DeviceHotplug(DeviceHotplug),
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

/// Which `navigator.serial` edge fired.
///
/// Both are "go look again", not "here is a port": the browser's listeners are
/// argument-free, so the effects layer answers each by sweeping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceHotplug {
    /// A granted port appeared. Sweep for grants not yet attached.
    ///
    /// ⚠️ Brave revokes grants on reload; Chrome persists them. An empty
    /// sweep is an ordinary answer.
    Connected,
    /// A port left. Detach the links that stopped being open.
    Disconnected,
}

impl StudioCommand {
    /// Whether this command is a refresh tick (used by tick coalescing).
    pub fn is_refresh_tick(&self) -> bool {
        matches!(self, StudioCommand::RefreshTick)
    }
}
