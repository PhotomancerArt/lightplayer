//! Derive macros for LightPlayer slot records and values.
//!
//! `lpc-slot-macros` is the proc-macro side of the slot data model. It keeps
//! Rust-authored records ergonomic while still making them available through
//! the dynamic slot interfaces used by the engine, wire sync, and UI.
//!
//! The main derive is [`SlotRecord`](derive@SlotRecord). It turns a named-field
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
//! use lpc_model::SlotRecord;
//!
//! #[derive(SlotRecord)]
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
//! pub param_defs: SlotMap<String, ShaderParamDef>,
//! ```
//!
//! Supported container attributes:
//!
//! - No container marker is required for a slot-modeled type; `SlotRecord`
//!   derives static shape support for every record.
//! - `#[slot(shape_id = "...")]`: override the generated static shape id.
//! Build-time slot-view generation discovers every `SlotRecord` and emits the
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
//!   reference another registered shape.

use proc_macro::TokenStream;

mod attr;
mod record;
mod value;

#[proc_macro_derive(SlotRecord, attributes(slot))]
pub fn derive_slot_record(input: TokenStream) -> TokenStream {
    record::derive(input.into()).into()
}

#[proc_macro_derive(SlotValue, attributes(slot_value))]
pub fn derive_slot_value(input: TokenStream) -> TokenStream {
    value::derive(input.into()).into()
}
