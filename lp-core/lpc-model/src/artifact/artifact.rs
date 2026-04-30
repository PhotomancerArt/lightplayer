use crate::Slot;

/// Metadata for a **versioned, on-disk** LightPlayer artifact: pattern, effect,
/// transition, stack, live, or playlist, each with its own `KIND` string and
/// schema `CURRENT_VERSION` (`docs/roadmaps/2026-04-22-lp-domain/overview.md`
/// — crate layout and `schema_version` story).
///
/// Typed deserialize, JSON Schema bounds, and migration wiring come in
/// M5+ (`// TODO` on this trait).
pub trait Artifact {
    /// TOML/JSON `kind` discriminator and file extension family (e.g. `"pattern"`).
    const KIND: &'static str;
    /// Breaking-schema bump only; see `single schema_version` in `overview.md` compatibility model.
    const CURRENT_VERSION: u32;

    /// On-disk `schema_version` field after load (validated against [`CURRENT_VERSION`](Self::CURRENT_VERSION) by the loader).
    fn schema_version(&self) -> u32;

    /// Visit every top-level [`Slot`] this artifact owns for load-time default materialization.
    ///
    /// Visuals with a `[params]` table walk the inner params-table root [`Slot`];
    /// nested fields are reached via [`Slot::default_value`](crate::prop::shape::Slot::default_value).
    fn walk_slots<F: FnMut(&Slot)>(&self, _f: F) {}
}
