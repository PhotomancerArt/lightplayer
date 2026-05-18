//! Authored slot bindings.
//!
//! Bindings declare how a node slot participates in the graph outside the
//! node's own authored config. A consumed slot binds from a `source`; a produced
//! slot binds to a `target`. The endpoint strings used in TOML are parsed into
//! semantic Rust forms here so loaders and runtimes do not re-parse text.

mod binding_def;
mod binding_defs;
mod binding_endpoint;
mod bus_slot_ref;
mod node_slot_ref;

pub use crate::slot_views::BindingDefView;
pub use binding_def::{BindingDef, BindingDefError};
pub use binding_defs::BindingDefs;
pub use binding_endpoint::{BindingRef, BindingRefError};
pub use bus_slot_ref::{BusSlotRef, BusSlotRefError};
pub use node_slot_ref::{NodeSlotRef, NodeSlotRefError};
