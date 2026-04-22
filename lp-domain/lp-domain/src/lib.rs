//! LightPlayer domain model.
//!
//! See [`docs/design/lightplayer/quantity.md`](../../../docs/design/lightplayer/quantity.md)
//! for the canonical Quantity-model spec this crate implements.
//!
//! ## schemars fallback chain (when a derive misbehaves)
//!
//! Every public type in this crate derives `schemars::JsonSchema` behind
//! the `schema-gen` feature. If a future derive fails (recursive-type
//! cycle, generic that schemars can't introspect, lifetime issue):
//!
//! 1. **Manual derive impl.** Hand-write `impl JsonSchema for T` returning
//!    a `schemars::schema::SchemaObject` that mirrors the serde shape.
//! 2. **Hand-written schema.** Drop the derive and ship a `pub fn
//!    <type>_schema() -> RootSchema` constructor that the codegen tool
//!    calls explicitly.
//! 3. **Drop schemars for the type.** As a last resort: remove the derive
//!    and document the type as "not part of the on-disk surface" so M4's
//!    codegen tool can skip it. M2 should not need this fallback for any
//!    type — if you find yourself reaching for it, stop and report.
//!
//! The smoke tests in `schema_gen_smoke.rs` catch broken derives early.

#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod binding;
pub mod constraint;
pub mod error;
pub mod kind;
pub mod node;
pub mod presentation;
pub mod schema;
pub mod shape;
pub mod types;
pub mod value_spec;

#[cfg(feature = "schema-gen")]
mod schema_gen_smoke;

pub use error::DomainError;
pub use lps_shared::LpsValueF32 as LpsValue;
pub use lps_shared::{LpsType, TextureBuffer, TextureStorageFormat};
