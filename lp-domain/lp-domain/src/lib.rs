//! LightPlayer **domain** crate: identity, addressing, and the **Quantity model**
//! ([`Kind`](kind::Kind), [`Constraint`](constraint::Constraint), [`Shape`](shape::Shape)/[`Slot`](shape::Slot), [`ValueSpec`](value_spec::ValueSpec), [`Binding`](binding::Binding), [`Presentation`](presentation::Presentation)).
//!
//! # Layered model
//!
//! The stack is intentionally layered (see `docs/design/lightplayer/quantity.md` §0, §1, and §2):
//!
//! 1. **[`LpsValue`]** — raw structural data (bytes, no semantics). From the `lps_shared` dependency.
//! 2. **[`LpsType`]** — the structural type of an [`LpsValue`]. From the `lps_shared` dependency.
//! 3. **[`Kind`](kind::Kind)** — semantic identity (e.g. `Kind::Frequency`, `Kind::Color`, `Kind::Texture`). This crate. Orthogonal to storage: one [`Kind`](kind::Kind) maps to a fixed storage recipe.
//! 4. **[`Constraint`](constraint::Constraint)** — which values are *legal* in a slot (range, choices). Domain truth, not a UI afterthought. See `quantity.md` §5.
//! 5. **[`Shape`](shape::Shape) and [`Slot`](shape::Slot)** — recursive composition (structural shape plus labels, bus [`Binding`](binding::Binding), and [`Presentation`](presentation::Presentation)). See `quantity.md` §6. Defaults for composed shapes are carried on [`Shape`](shape::Shape), not a separate `default` field on [`Slot`](shape::Slot) (M2, “Option A”; see `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md`).
//!
//! # Environment and features
//!
//! - **Default build:** `no_std` with the `alloc` crate, for embedded class targets (e.g. ESP32-C6), matching the roadmap (`docs/roadmaps/2026-04-22-lp-domain/overview.md`).
//! - **`std`:** optional `std` wiring (e.g. `lpfs` / `toml` with `std`) for host-side and tooling.
//! - **`schema-gen`:** enables schemars `JsonSchema` on model types and pulls `schemars` through `lps-shared` for host-side JSON Schema generation. Schemars’ own **codegen** is std-only; use this feature where tooling runs (`docs/roadmaps/2026-04-22-lp-domain/overview.md` — “Risks”, `schemars`).
//!
//! # Design references
//!
//! - **`docs/design/lightplayer/quantity.md`** — primary Quantity spec.
//! - **`docs/roadmaps/2026-04-22-lp-domain/overview.md`** — domain layer, artifacts, and migration story.
//! - **`docs/roadmaps/2026-04-22-lp-domain/notes-quantity.md`** — historical “why” for Quantity decisions.
//! - **M2 milestone:** `docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`, `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md`, `summary.md`.
//!
//! # schemars fallback (when a derive misbehaves)
//!
//! Public types in this crate derive or implement the schemars `JsonSchema` trait behind `schema-gen` where possible. If a future derive fails (recursive type cycle, generic introspection, lifetimes), use this order:
//!
//! 1. **Manual `JsonSchema` impl** — return a `schemars::schema::SchemaObject` that mirrors the serde shape.
//! 2. **Hand-written schema** — a `pub fn <type>_schema() -> RootSchema` that codegen calls explicitly.
//! 3. **Last resort** — remove the derive and document the type as not part of the on-disk or generated schema surface.
//!
//! M2 is not expected to need (3) for any model type. The smoke tests in `schema_gen_smoke` catch broken `schema_for!` / derives early.

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

/// Cross-cutting error for [`Node`](node::Node) property access and related domain operations.
pub use error::DomainError;
/// Shader-facing structural type system (mirrors [`LpsValue`]); shared with the GLSL/compilation stack.
pub use lps_shared::LpsType;
/// Canonical structural **value** type for the engine and tooling: `lps_shared::LpsValueF32` re-exported for convenience.
pub use lps_shared::LpsValueF32 as LpsValue;
/// Opaque texture pixel storage (lives beside handle values in the GPU/loader story).
pub use lps_shared::TextureBuffer;
/// Texture format id for [`Kind::Texture`](kind::Kind::Texture) storage; from `lps_shared`. See `docs/design/lightplayer/quantity.md` §3 (storage table).
pub use lps_shared::TextureStorageFormat;
