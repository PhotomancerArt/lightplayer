//! "Added to <device>": what a USB connect installed on its own (plan D6).
//!
//! Physical connection is access: plugging a device in by USB silently adds
//! this browser's key, and the account key and account passwords when signed
//! in — whichever are missing. There is no prompt; the shell raises a toast
//! naming what was added, with Undo ([`super::AccessCommand::UndoAutoAdd`]),
//! which removes exactly those entries.
//!
//! It rides the studio view as state. `generation` counts every add in this
//! tab, so the shell raises the toast once per add even when two adds name
//! the same things (the same pattern as the fork toast).

use lpa_devices::identity::DeviceId;

/// See the module doc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessAdded {
    pub device: DeviceId,
    /// The labels added, in the order they were ("Yona's MacBook", "Yona's
    /// account").
    pub names: Vec<String>,
    /// Monotonic per tab; a new value is a new toast.
    pub generation: u64,
}
