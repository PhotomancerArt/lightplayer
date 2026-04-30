//! Wire-facing tree sync types.

mod wire_child_kind;
mod wire_entry_state;
mod wire_tree_delta;

pub use wire_child_kind::{SlotIdx, WireChildKind};
pub use wire_entry_state::WireEntryState;
pub use wire_tree_delta::WireTreeDelta;
