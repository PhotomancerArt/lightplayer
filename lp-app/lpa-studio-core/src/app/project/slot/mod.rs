//! Project slot controller-domain types.
//!
//! Slots are addressed under a project node, a slot root such as `def` or
//! `state`, and a structured [`lpc_model::SlotPath`]. Studio creates recursive
//! slot controllers for containers and leaves so expansion, binding, dirty
//! state, DTO projection, and future edits have addressable homes.

pub mod project_slot_address;
pub mod project_slot_root;
pub mod slot_controller;

pub use project_slot_address::ProjectSlotAddress;
pub use project_slot_root::ProjectSlotRoot;
pub use slot_controller::{SlotController, SlotControllerState, SlotKind};
