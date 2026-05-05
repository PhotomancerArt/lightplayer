//! Slot identity and value-reference model.
//!
//! A slot is a named location owned by a node or bus. A [`ValuePath`] navigates
//! inside the value exposed at that slot; it is not part of the slot identity.

mod slot_name;
mod slot_owner;
mod slot_ref;
mod value_ref;

pub use slot_name::{SlotName, SlotNameError};
pub use slot_owner::SlotOwner;
pub use slot_ref::SlotRef;
pub use value_ref::ValueRef;
