# Future Work

## Field-Presence Enum Discrimination

- **Idea:** Add a second enum encoding where a unique `#[slot(key)]` field inside each record variant selects the active variant.
- **Why not now:** It requires per-variant key metadata, derive validation for unique keys, and ambiguity handling. External enum encoding is smaller and immediately useful.
- **Useful context:** User prefers this design for API extensibility in config-like shapes, because non-key fields can be added later without changing the outer namespace.

## Broader Rename Policies

- **Idea:** Support more Serde-like rename policies such as `kebab-case`, `SCREAMING_SNAKE_CASE`, and field-level rename-all.
- **Why not now:** `snake_case` covers the immediate multi-word variant need and keeps the first implementation focused.
- **Useful context:** Existing slot names are ASCII identifier-like, so any broader policy should preserve `SlotName` validity.

## Shader Source Artifact Migration

- **Idea:** Replace shader `glsl_path` fields with an external enum source spec such as `glsl.file = "compute.glsl"` or `glsl.inline = "..."`, then resolve both through the artifact manager.
- **Why not now:** This plan is intentionally limited to slot-system support. Shader source artifacts and live reload should be planned separately.
- **Useful context:** The desired long-term boundary is for shader nodes to ask for source snapshots and revisions without knowing whether source came from a file, inline TOML, module, or library.
