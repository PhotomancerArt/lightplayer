/// Metadata for a **versioned, on-disk** LightPlayer artifact: pattern, effect,
/// transition, stack, live, or playlist, each with its own `KIND` string and
/// schema `CURRENT_VERSION` (`docs/roadmaps/2026-04-22-lp-domain/overview.md`
/// — crate layout and `schema_version` story).
///
/// Typed deserialize, JSON Schema bounds, and migration wiring come in
/// M5+ (`// TODO` on this trait).
pub trait SrcArtifact {
    /// TOML/JSON `kind` discriminator and file extension family (e.g. `"pattern"`).
    const KIND: &'static str;
    /// Breaking-schema bump only; see `single schema_version` in `overview.md` compatibility model.
    const CURRENT_VERSION: u32;

    /// On-disk `schema_version` field after load (validated against [`CURRENT_VERSION`](Self::CURRENT_VERSION) by the loader).
    fn schema_version(&self) -> u32;
}
