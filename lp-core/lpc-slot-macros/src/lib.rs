//! Derive macros for LightPlayer slot records and values.
//!
//! `lpc-slot-macros` is the proc-macro side of the slot data model. It keeps
//! Rust-authored records ergonomic while still making them available through
//! the dynamic slot interfaces used by the engine, wire sync, and UI.
//!
//! The main derive is [`Slotted`](derive@Slotted). It turns a named-field
//! struct into a slot record by generating:
//!
//! - `SlotRecordShape`, so the type can describe its slot shape.
//! - `SlotRecordAccess`, so generic code can walk field data by index.
//! - `SlotMapValueAccess` and `FieldSlot`, so the record can be nested inside
//!   other slot records or typed maps.
//! - `SlotAccess`, `StaticSlotShape`, and `StaticSlotAccess`, making the struct
//!   a registry-addressable slot object.
//!
//! A minimal slot record looks like:
//!
//! ```ignore
//! use lpc_model::Slotted;
//!
//! #[derive(Slotted)]
//! pub struct TextureDef {
//!     pub size: Dim2uSlot,
//! }
//! ```
//!
//! By default, field shape and access are inferred from `FieldSlot`. Use field
//! attributes when the Rust type needs an explicit structural shape:
//!
//! ```ignore
//! #[slot(name = "params", map(key = "string", value_ref = "shader.param_def"))]
//! pub consumed_slots: SlotMap<String, ShaderSlotDef>,
//! ```
//!
//! Supported container attributes:
//!
//! - No container marker is required for a slot-modeled type; `Slotted`
//!   derives static shape support for every record.
//! - `#[slot(shape_id = "...")]`: override the generated static shape id.
//! Build-time slot-view generation discovers every `Slotted` and emits the
//! corresponding `*View` type.
//!
//! Supported field attributes:
//!
//! - `#[slot(name = "...")]`: use a different slot field name.
//! - `#[slot(value = expr)]`: use an explicit `LpType` value leaf shape.
//! - `#[slot(leaf = expr)]`: use an explicit semantic slot-value shape.
//! - `#[slot(record)]`: force nested record shape/access.
//! - `#[slot(option_ref = "...")]`: shape an option around another registered shape.
//! - `#[slot(map(key = "...", value_ref = "..."))]`: shape a map whose values
//!   reference another shape root.
//! - `#[slot(consumed)]`: mark the field as a consumed dataflow slot.
//! - `#[slot(produced)]`: mark the field as a produced dataflow slot.
//! - `#[slot(merge = "latest" | "error" | "by_key")]`: set the receiver-owned
//!   merge policy for aggregate consumed slots.

use proc_macro::TokenStream;

mod attr;
mod slotted;
mod slotted_enum;
mod slotted_record;
mod slotted_wrapper;
mod value;

#[proc_macro_derive(Slotted, attributes(slot, default))]
pub fn derive_slotted(input: TokenStream) -> TokenStream {
    slotted::derive(input.into()).into()
}

#[proc_macro_derive(SlotValue, attributes(slot_value))]
pub fn derive_slot_value(input: TokenStream) -> TokenStream {
    value::derive(input.into()).into()
}
