//! Client-side mirror for generic slot sync and mutation.

mod apply;
mod mirror;
mod pending;

pub use apply::SlotMirrorError;
pub use mirror::SlotMirrorView;
pub use pending::PendingSlotMutation;
