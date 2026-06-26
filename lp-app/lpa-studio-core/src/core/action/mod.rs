//! Data-driven action models for controller operations.
//!
//! `UiAction` pairs a controller operation with render metadata. Controllers
//! create actions from their own operation enums, and web components render the
//! metadata without needing to know the operation type.

pub mod action;
pub mod action_confirmation;
pub mod action_enablement;
pub mod action_meta;
pub mod action_priority;
pub mod actions;
